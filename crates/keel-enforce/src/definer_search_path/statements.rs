//! The statements that create, alter or drop a routine, read from one SQL file.

use super::lex::{lex, Kind, Token};

/// A routine as a statement names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RoutineRef {
    pub schema: Option<String>,
    pub name: String,
    /// Canonical input argument types; `None` when the statement lists none.
    pub args: Option<Vec<String>>,
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

/// Every routine statement of `sql`, in order.
pub(super) fn statements(sql: &str) -> Vec<Statement> {
    let tokens = lex(sql);
    let mut found = Vec::new();
    for chunk in tokens.split(|token| token.is_punct(';')) {
        if chunk.first().is_some_and(|token| token.is_word("do")) {
            let body = chunk.iter().find(|token| token.kind == Kind::Str);
            found.extend(
                body.into_iter()
                    .flat_map(|body| block_statements(&body.text)),
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

/// Routine statements anywhere in a procedural block, each conditional.
fn block_statements(body: &str) -> Vec<Statement> {
    let tokens = lex(body);
    let mut found = Vec::new();
    for chunk in tokens.split(|token| token.is_punct(';')) {
        let hit = (0..chunk.len()).find_map(|start| event(&chunk[start..]));
        found.extend(hit.map(|event| Statement {
            event,
            offset: 0,
            conditional: true,
        }));
    }
    found
}

fn event(tokens: &[Token]) -> Option<Event> {
    let mut cursor = Cursor { tokens, at: 0 };
    let verb = cursor.next()?;
    if verb.is_word("create") {
        if cursor.eat_word("or") {
            cursor.eat_word("replace");
        }
        cursor.eat_any(&["function", "procedure"])?;
        let routine = cursor.routine()?;
        let (definer, path) = cursor.create_clauses();
        Some(Event::Create {
            routine,
            definer,
            path,
        })
    } else if verb.is_word("alter") {
        cursor.eat_any(&["function", "procedure", "routine"])?;
        let routine = cursor.routine()?;
        let actions = cursor.alter_actions();
        Some(Event::Alter { routine, actions })
    } else if verb.is_word("drop") {
        cursor.eat_any(&["function", "procedure", "routine"])?;
        let mut routines = vec![cursor.routine()?];
        while cursor.eat_punct(',') {
            routines.push(cursor.routine()?);
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

    fn eat_any(&mut self, words: &[&str]) -> Option<()> {
        words.iter().any(|word| self.eat_word(word)).then_some(())
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
    fn routine(&mut self) -> Option<RoutineRef> {
        if self.eat_word("if") {
            self.eat_word("exists");
        }
        let mut parts = vec![self.next()?.ident()?.to_string()];
        while self.eat_punct('.') {
            parts.push(self.next()?.ident()?.to_string());
        }
        let name = parts.pop()?;
        let args = self.eat_punct('(').then(|| {
            let group = self.group();
            group
                .split(|token| token.is_punct(','))
                .filter(|argument| !argument.is_empty())
                .filter_map(super::types::input_type)
                .collect()
        });
        Some(RoutineRef {
            schema: parts.pop(),
            name,
            args,
        })
    }

    /// The clauses of `CREATE FUNCTION` after its argument list.
    fn create_clauses(&mut self) -> (bool, Setting) {
        let (mut definer, mut path) = (false, Setting::Unset);
        while let Some(token) = self.next() {
            if token.is_word("begin") || token.is_word("return") {
                break; // a SQL-standard body: no clause follows it
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
            } else if token.is_word("set") && self.eat_word("schema") {
                self.ident().map(Action::SetSchema)
            } else if token.is_word("set") {
                self.search_path().map(Action::Path)
            } else if token.is_word("reset") {
                let all = self.eat_word("all");
                (all || self.eat_search_path()).then_some(Action::Path(Setting::Unset))
            } else if token.is_word("rename") && self.eat_word("to") {
                self.ident().map(Action::Rename)
            } else if token.is_word("owner") && self.eat_word("to") {
                self.at += 1; // a role name is never a clause
                None
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
