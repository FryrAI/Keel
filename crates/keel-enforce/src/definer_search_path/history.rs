//! Replays routine statements in the order a migration runner applies them
//! and judges what is left: each live routine's effective `search_path`.

use std::collections::HashMap;

use super::statements::{statements, Action, Args, Event, RoutineKind, RoutineRef, Setting};

/// A statement's position: index into the replayed sources, 1-based line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Site {
    pub source: usize,
    pub line: u32,
}

/// One flagged `CREATE` statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Finding {
    /// The routine's name as created.
    pub name: String,
    /// Line of the `CREATE` keyword.
    pub line: u32,
    /// The effective list, or `None` when no `search_path` is set.
    pub list: Option<Vec<String>>,
    /// The later statement that set or reset it, if one did.
    pub changed_at: Option<Site>,
}

struct Routine {
    schema: Option<String>,
    name: String,
    args: Args,
    procedure: bool,
    definer: bool,
    path: Setting,
    changed_at: Option<Site>,
    created_as: String,
    created_at: Site,
    /// A statement that may or may not have run touched this routine.
    undetermined: bool,
}

/// The live routines called as `routine` is. An unqualified name means
/// `public`, where the default path creates and finds it. `or_elsewhere` lets
/// it mean another schema when `public` has no such routine: the session's own
/// path decides, and no file records it.
fn named(live: &[Routine], routine: &RoutineRef, or_elsewhere: bool) -> Vec<usize> {
    let schema = |schema: &Option<String>| schema.clone().unwrap_or_else(|| "public".to_string());
    let called: Vec<usize> = (0..live.len())
        .filter(|index| live[*index].name == routine.name)
        .collect();
    let exact: Vec<usize> = called
        .iter()
        .copied()
        .filter(|index| schema(&live[*index].schema) == schema(&routine.schema))
        .collect();
    if !exact.is_empty() || !or_elsewhere {
        return exact;
    }
    called
        .into_iter()
        .filter(|index| live[*index].schema.is_none() || routine.schema.is_none())
        .collect()
}

/// The findings of `sources` (file texts in applied order), keyed by source index.
pub(super) fn findings(sources: &[&str]) -> HashMap<usize, Vec<Finding>> {
    let mut live: Vec<Routine> = Vec::new();
    for (source, sql) in sources.iter().enumerate() {
        // Statements come in file order, so lines are counted once.
        let (mut counted, mut line) = (0, 1);
        for statement in statements(sql) {
            line += sql[counted..statement.offset].matches('\n').count() as u32;
            counted = statement.offset;
            let site = Site { source, line };
            let (hit, certain) = targets(&live, &statement.event);
            if statement.conditional || !certain {
                for index in hit {
                    live[index].undetermined = true;
                }
            } else {
                apply(&mut live, statement.event, &hit, site);
            }
        }
    }

    let mut found: HashMap<usize, Vec<Finding>> = HashMap::new();
    for routine in live {
        let list = match routine.path {
            _ if !routine.definer || routine.undetermined => continue,
            Setting::FromCurrent => continue,
            Setting::List(list) if ends_with_pg_temp(&list) => continue,
            Setting::List(list) => Some(list),
            Setting::Unset => None,
        };
        found
            .entry(routine.created_at.source)
            .or_default()
            .push(Finding {
                name: routine.created_as,
                line: routine.created_at.line,
                list,
                changed_at: routine.changed_at,
            });
    }
    found
}

/// PostgreSQL keeps a repeated schema where it first appears, so the last
/// schema searched is the last entry of the de-duplicated list.
fn ends_with_pg_temp(list: &[String]) -> bool {
    let last_new = list
        .iter()
        .enumerate()
        .rev()
        .find(|(index, schema)| !list[..*index].contains(schema));
    last_new.is_some_and(|(_, schema)| schema == "pg_temp")
}

fn apply(live: &mut Vec<Routine>, event: Event, hit: &[usize], site: Site) {
    match event {
        Event::Create {
            routine,
            definer,
            path,
        } => {
            // A replaced definition is gone: every property is assigned anew.
            remove(live, hit);
            live.push(Routine {
                schema: routine.schema,
                created_as: routine.name.clone(),
                name: routine.name,
                args: routine.args.unwrap_or_default(),
                procedure: routine.kind == RoutineKind::Procedure,
                definer,
                path,
                changed_at: None,
                created_at: site,
                undetermined: false,
            });
        }
        Event::Alter { actions, .. } => {
            for &index in hit {
                for action in &actions {
                    let routine = &mut live[index];
                    match action.clone() {
                        Action::Path(path) => {
                            routine.path = path;
                            routine.changed_at = Some(site);
                        }
                        Action::Definer(definer) => routine.definer = definer,
                        Action::Rename(name) => routine.name = name,
                        Action::SetSchema(schema) => routine.schema = Some(schema),
                    }
                }
            }
        }
        Event::Drop(_) => remove(live, hit),
    }
}

fn remove(live: &mut Vec<Routine>, indices: &[usize]) {
    let mut index = 0;
    live.retain(|_| {
        index += 1;
        !indices.contains(&(index - 1))
    });
}

/// The live routines `event` names, and whether that reading is certain.
fn targets(live: &[Routine], event: &Event) -> (Vec<usize>, bool) {
    match event {
        // A definition replaces the routine of its own signature: the input
        // types, for functions and procedures alike.
        Event::Create { routine, .. } => {
            let inputs = routine.args.as_ref().map(|args| &args.inputs);
            let mut same = named(live, routine, false);
            same.retain(|index| Some(&live[*index].args.inputs) == inputs);
            (same, true)
        }
        Event::Alter { routine, .. } => resolve(live, routine),
        Event::Drop(routines) => {
            let resolved: Vec<_> = routines.iter().map(|one| resolve(live, one)).collect();
            let certain = resolved.iter().all(|(_, certain)| *certain);
            (
                resolved.into_iter().flat_map(|(hit, _)| hit).collect(),
                certain,
            )
        }
    }
}

/// The live routines `routine` names, as PostgreSQL looks them up: among those
/// of the statement's kind, by name alone without an argument list, else by
/// the input types. A list with no mode marker after `PROCEDURE` or `ROUTINE`
/// may also be the full list, `OUT` types included.
fn resolve(live: &[Routine], routine: &RoutineRef) -> (Vec<usize>, bool) {
    let mut same_arity = named(live, routine, true);
    same_arity.retain(|index| match routine.kind {
        RoutineKind::Function => !live[*index].procedure,
        RoutineKind::Procedure => live[*index].procedure,
        RoutineKind::Routine => true,
    });
    let Some(args) = &routine.args else {
        return (same_arity, true);
    };
    let or_full = routine.kind != RoutineKind::Function && !args.marked;
    same_arity.retain(|index| {
        let have = &live[*index].args;
        have.inputs.len() == args.inputs.len() || (or_full && have.all.len() == args.all.len())
    });
    // One candidate is the target however its types are spelled: the statement
    // would not have applied otherwise. `IF EXISTS` applies either way, so it
    // names a routine by its types alone.
    if same_arity.len() < 2 && !routine.if_exists {
        return (same_arity, true);
    }
    let same_types: Vec<usize> = same_arity
        .iter()
        .copied()
        .filter(|index| {
            let have = &live[*index].args;
            have.inputs == args.inputs || (or_full && have.all == args.all)
        })
        .collect();
    // Several candidates and no spelling matches: any of them may be meant.
    if same_types.is_empty() && !routine.if_exists {
        (same_arity, false)
    } else {
        (same_types, true)
    }
}
