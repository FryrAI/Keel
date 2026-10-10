//! The statements that create, alter or drop a routine, read from one SQL file.

use super::lex::{lex, Kind, Token};

/// A routine as a statement names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RoutineRef {
    pub schema: Option<String>,
    pub name: String,
    /// `None` when the statement gives no argument list.
    pub args: Option<Args>,
    /// Named with `PROCEDURE` or `ROUTINE`: PostgreSQL then also accepts the
    /// full argument list, `OUT` types included, and identifies a procedure
    /// by it.
    pub by_full_list: bool,
    /// `IF EXISTS`: naming no routine is not an error.
    pub if_exists: bool,
}

/// A routine's canonical argument types.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Args {
    /// What identifies a function: `OUT` arguments left out.
    pub inputs: Vec<String>,
    /// Every argument.
    pub all: Vec<String>,
}

impl Args {
    /// Whether the list `named` in a statement has this routine's arity.
    pub fn same_arity(&self, named: &Args, by_full_list: bool) -> bool {
        self.inputs.len() == named.inputs.len()
            || (by_full_list && self.all.len() == named.all.len())
    }

    /// Whether the list `named` in a statement spells this routine's types.
    pub fn same_types(&self, named: &Args, by_full_list: bool) -> bool {
        self.inputs == named.inputs || (by_full_list && self.all == named.all)
    }
}

/// A routine's stored `search_path` setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Setting {
    Unset,
    List(Vec<String>),
    /// `FROM CURRENT`: the creating session's value, which no file records.
    FromCurrent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Action {
    Path(Setting),
    Definer(bool),
    Rename(String),
    SetSchema(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Event {
    Create {
        routine: RoutineRef,
        definer: bool,
        path: Setting,
    },
    Alter {
        routine: RoutineRef,
        actions: Vec<Action>,
    },
    Drop(Vec<RoutineRef>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Statement {
    pub event: Event,
    /// Byte offset of the statement's first keyword.
    pub offset: usize,
    /// Inside a `DO` block: whether it ran is not in the file.
    pub conditional: bool,
}

/// Comment lines that open the section a forward run does not apply (dbmate,
/// goose, sql-migrate).
const DOWN_MARKERS: &[&str] = &["-- migrate:down", "-- +goose down", "-- +migrate down"];

/// The part of `sql` a forward migration run applies.
fn forward_part(sql: &str) -> &str {
    let mut end = 0;
    for line in sql.split_inclusive('\n') {
        // A directive is the marker alone, or the marker and its options.
        let lower = line.to_ascii_lowercase();
        let is_directive = |marker: &&str| {
            let rest = lower.strip_prefix(*marker);
            rest.is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
        };
        if DOWN_MARKERS.iter().any(is_directive) {
            return &sql[..end];
        }
        end += line.len();
    }
    sql
}

/// Every routine statement of `sql`, in order.
pub(super) fn statements(sql: &str) -> Vec<Statement> {
    let tokens = lex(forward_part(sql));
    let mut found = Vec::new();
    for chunk in tokens.split(|token| token.is_punct(';')) {
        if chunk.first().is_some_and(|token| token.is_word("do")) {
            let body = chunk.iter().find(|token| token.kind == Kind::Str);
            found.extend(
                body.into_iter()
                    .flat_map(|body| block_statements(&body.text, chunk[0].offset)),
            );
        } else if let Some(event) = event(chunk) {
            found.push(Statement {
                event,
                offset: chunk[0].offset,
                conditional: false,
            });
        }
    }
    found
}

/// Routine statements anywhere in the procedural block at `offset`, each conditional.
fn block_statements(body: &str, offset: usize) -> Vec<Statement> {
    let tokens = lex(body);
    let mut found = Vec::new();
    for chunk in tokens.split(|token| token.is_punct(';')) {
        let hit = (0..chunk.len()).find_map(|start| event(&chunk[start..]));
        found.extend(hit.map(|event| Statement {
            event,
            offset,
            conditional: true,
        }));
    }
    found
}

/// The arguments of a parenthesised list: split at its own commas only, not
/// at those of a type modifier or a default expression.
fn arguments(group: &[Token]) -> Vec<&[Token]> {
    let (mut found, mut start, mut depth) = (Vec::new(), 0, 0usize);
    for (index, token) in group.iter().enumerate() {
        if token.is_punct('(') || token.is_punct('[') {
            depth += 1;
        } else if token.is_punct(')') || token.is_punct(']') {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && token.is_punct(',') {
            found.push(&group[start..index]);
            start = index + 1;
        }
    }
    found.push(&group[start..]);
    found.retain(|argument| !argument.is_empty());
    found
}

fn event(tokens: &[Token]) -> Option<Event> {
    let mut cursor = Cursor { tokens, at: 0 };
    let verb = cursor.next()?;
    if verb.is_word("create") {
        if cursor.eat_word("or") {
            cursor.eat_word("replace");
        }
        let routine = cursor
            .routine_kind()
            .and_then(|kind| cursor.routine(kind))?;
        let (definer, path) = cursor.create_clauses();
        Some(Event::Create {
            routine,
            definer,
            path,
        })
    } else if verb.is_word("alter") {
        let routine = cursor
            .routine_kind()
            .and_then(|kind| cursor.routine(kind))?;
        let actions = cursor.alter_actions();
        Some(Event::Alter { routine, actions })
    } else if verb.is_word("drop") {
        let kind = cursor.routine_kind()?;
        let mut routines = vec![cursor.routine(kind)?];
        while cursor.eat_punct(',') {
            routines.push(cursor.routine(kind)?);
        }
        Some(Event::Drop(routines))
    } else {
        None
    }
}

struct Cursor<'a> {
    tokens: &'a [Token],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<&'a Token> {
        let token = self.peek()?;
        self.at += 1;
        Some(token)
    }

    fn eat_word(&mut self, word: &str) -> bool {
        let hit = self.peek().is_some_and(|token| token.is_word(word));
        self.at += usize::from(hit);
        hit
    }

    /// The routine keyword: false for `FUNCTION`, true for `PROCEDURE` and
    /// `ROUTINE`, which name a routine by its full argument list too.
    fn routine_kind(&mut self) -> Option<bool> {
        if self.eat_word("function") {
            return Some(false);
        }
        (self.eat_word("procedure") || self.eat_word("routine")).then_some(true)
    }

    fn eat_punct(&mut self, punct: char) -> bool {
        let hit = self.peek().is_some_and(|token| token.is_punct(punct));
        self.at += usize::from(hit);
        hit
    }

    /// The tokens of a parenthesised group whose `(` was just consumed.
    fn group(&mut self) -> &'a [Token] {
        let (start, mut depth) = (self.at, 1usize);
        while let Some(token) = self.next() {
            depth += usize::from(token.is_punct('('));
            depth -= usize::from(token.is_punct(')'));
            if depth == 0 {
                return &self.tokens[start..self.at - 1];
            }
        }
        &self.tokens[start..]
    }

    /// `[IF EXISTS] [schema.]name [(arguments)]`.
    fn routine(&mut self, by_full_list: bool) -> Option<RoutineRef> {
        let if_exists = self.eat_word("if") && self.eat_word("exists");
        let mut parts = vec![self.next()?.ident()?.to_string()];
        while self.eat_punct('.') {
            parts.push(self.next()?.ident()?.to_string());
        }
        let name = parts.pop()?;
        let args = self.eat_punct('(').then(|| {
            let mut args = Args::default();
            for argument in arguments(self.group()) {
                let (name, input) = super::types::argument_type(argument);
                args.all.push(name.clone());
                if input {
                    args.inputs.push(name);
                }
            }
            args
        });
        Some(RoutineRef {
            schema: parts.pop(),
            name,
            args,
            by_full_list,
            if_exists,
        })
    }

    /// The clauses of `CREATE FUNCTION` after its argument list.
    fn create_clauses(&mut self) -> (bool, Setting) {
        let (mut definer, mut path) = (false, Setting::Unset);
        while let Some(token) = self.next() {
            if token.is_word("begin") {
                break; // `BEGIN ATOMIC`: the body, and no clause after it
            } else if token.is_punct('(') {
                self.group();
            } else if token.is_word("security") {
                definer = self.security().unwrap_or(definer);
            } else if token.is_word("set") {
                path = self.search_path().unwrap_or(path);
            }
        }
        (definer, path)
    }

    fn alter_actions(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        while let Some(token) = self.next() {
            let action = if token.is_word("security") {
                self.security().map(Action::Definer)
            } else if token.is_word("set") {
                match self.search_path() {
                    Some(path) => Some(Action::Path(path)),
                    None if self.eat_word("schema") => self.ident().map(Action::SetSchema),
                    None => None,
                }
            } else if token.is_word("reset") {
                let all = self.eat_word("all");
                (all || self.eat_search_path()).then_some(Action::Path(Setting::Unset))
            } else if token.is_word("rename") && self.eat_word("to") {
                self.ident().map(Action::Rename)
            } else {
                None
            };
            actions.extend(action);
        }
        actions
    }

    fn ident(&mut self) -> Option<String> {
        Some(self.next()?.ident()?.to_string())
    }

    /// After `SECURITY`: true for `DEFINER`, false for `INVOKER`.
    fn security(&mut self) -> Option<bool> {
        if self.eat_word("definer") {
            Some(true)
        } else {
            self.eat_word("invoker").then_some(false)
        }
    }

    fn eat_search_path(&mut self) -> bool {
        let hit = self
            .peek()
            .and_then(Token::ident)
            .is_some_and(|name| name.eq_ignore_ascii_case("search_path"));
        self.at += usize::from(hit);
        hit
    }

    /// After `SET`: the setting, when the parameter is `search_path`.
    fn search_path(&mut self) -> Option<Setting> {
        // `SET SCHEMA 'name'` is PostgreSQL's alias for a one-schema list.
        if let [schema, name, ..] = &self.tokens[self.at..] {
            if schema.is_word("schema") && name.kind == Kind::Str {
                self.at += 2;
                return Some(Setting::List(vec![name.text.clone()]));
            }
        }
        if !self.eat_search_path() {
            return None;
        }
        if self.eat_word("from") {
            return self.eat_word("current").then_some(Setting::FromCurrent);
        }
        if !self.eat_word("to") && !self.eat_punct('=') {
            return None;
        }
        if self.eat_word("default") {
            return Some(Setting::Unset);
        }
        // Each list element is one schema name: an identifier, or a string
        // constant taken verbatim (`'a, b'` names a single schema).
        let mut schemas = Vec::new();
        while let Some(token) = self.peek().filter(|token| token.kind != Kind::Punct) {
            schemas.push(token.text.clone());
            self.at += 1;
            if !self.eat_punct(',') {
                break;
            }
        }
        Some(Setting::List(schemas))
    }
}
