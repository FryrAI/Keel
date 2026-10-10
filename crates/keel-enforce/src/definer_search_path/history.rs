//! Replays routine statements in the order a migration runner applies them
//! and judges what is left: each live routine's effective `search_path`.

use std::collections::HashMap;

use super::statements::{statements, Action, Event, RoutineRef, Setting};

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
    args: Vec<String>,
    definer: bool,
    path: Setting,
    changed_at: Option<Site>,
    created_as: String,
    created_at: Site,
    /// A statement that may or may not have run touched this routine.
    undetermined: bool,
}

impl Routine {
    /// An unqualified name is resolved through the session's own path, which
    /// no file records: it matches the name in any schema.
    fn is_named(&self, routine: &RoutineRef) -> bool {
        self.name == routine.name
            && (self.schema.is_none() || routine.schema.is_none() || self.schema == routine.schema)
    }
}

/// The findings of `sources` (file texts in applied order), keyed by source index.
pub(super) fn findings(sources: &[&str]) -> HashMap<usize, Vec<Finding>> {
    let mut live: Vec<Routine> = Vec::new();
    for (source, sql) in sources.iter().enumerate() {
        for statement in statements(sql) {
            let line = 1 + sql[..statement.offset].matches('\n').count() as u32;
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
        // A definition replaces exactly the routine of its own signature.
        Event::Create { routine, .. } => {
            let same = (0..live.len())
                .filter(|index| live[*index].is_named(routine))
                .filter(|index| Some(&live[*index].args) == routine.args.as_ref());
            (same.collect(), true)
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

/// The live routines `routine` names, as PostgreSQL picks them: by name alone
/// without an argument list, else by the input argument types.
fn resolve(live: &[Routine], routine: &RoutineRef) -> (Vec<usize>, bool) {
    let named = (0..live.len()).filter(|index| live[*index].is_named(routine));
    let Some(args) = &routine.args else {
        return (named.collect(), true);
    };
    let same_arity: Vec<usize> = named
        .filter(|index| live[*index].args.len() == args.len())
        .collect();
    // One candidate is the target however its types are spelled. Several are
    // told apart by type; when no spelling matches, any of them may be meant.
    if same_arity.len() < 2 {
        return (same_arity, true);
    }
    let same_types: Vec<usize> = same_arity
        .iter()
        .copied()
        .filter(|index| live[*index].args == *args)
        .collect();
    if same_types.is_empty() {
        (same_arity, false)
    } else {
        (same_types, true)
    }
}
