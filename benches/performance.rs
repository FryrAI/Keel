use criterion::{black_box, criterion_group, criterion_main, Criterion};

use keel_core::hash::{compute_hash, compute_hash_disambiguated};
use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind};
use keel_parsers::treesitter::TreeSitterParser;
use std::path::Path;

// ---------------------------------------------------------------------------
// Hash computation benchmarks
// ---------------------------------------------------------------------------

fn bench_hash_computation(c: &mut Criterion) {
    let sig = "fn authenticate(token: &str, scope: &str) -> Result<User, AuthError>";
    let body = r#"let decoded = decode_jwt(token)?;
        if decoded.exp < now() { return Err(AuthError::Expired); }
        let user = db.find_user(decoded.sub)?;
        if !user.scopes.contains(scope) { return Err(AuthError::Forbidden); }
        Ok(user)"#;
    let doc = "Authenticates a user by validating JWT token and checking scope permissions.";

    c.bench_function("hash_small_function", |b| {
        b.iter(|| compute_hash(black_box(sig), black_box(body), black_box(doc)))
    });

    let large_body = body.repeat(50);
    c.bench_function("hash_large_function", |b| {
        b.iter(|| {
            compute_hash(
                black_box(sig),
                black_box(&large_body),
                black_box(doc),
            )
        })
    });

    c.bench_function("hash_disambiguated", |b| {
        b.iter(|| {
            compute_hash_disambiguated(
                black_box(sig),
                black_box(body),
                black_box(doc),
                black_box("src/auth/middleware.rs"),
            )
        })
    });
}

// ---------------------------------------------------------------------------
// Tree-sitter parsing benchmarks
// ---------------------------------------------------------------------------

const TYPESCRIPT_SOURCE: &str = r#"
import { Request, Response } from 'express';
import { authenticate } from './auth';
import { UserService } from './services/user';

interface User {
    id: string;
    name: string;
    email: string;
    role: 'admin' | 'user';
}

export class UserController {
    private userService: UserService;

    constructor(userService: UserService) {
        this.userService = userService;
    }

    async getUser(req: Request, res: Response): Promise<void> {
        const user = await this.userService.findById(req.params.id);
        if (!user) {
            res.status(404).json({ error: 'Not found' });
            return;
        }
        res.json(user);
    }

    async createUser(req: Request, res: Response): Promise<void> {
        const { name, email, role } = req.body;
        const user = await this.userService.create({ name, email, role });
        res.status(201).json(user);
    }

    async deleteUser(req: Request, res: Response): Promise<void> {
        await this.userService.delete(req.params.id);
        res.status(204).send();
    }
}

export function formatUser(user: User): string {
    return `${user.name} <${user.email}> (${user.role})`;
}
"#;

const PYTHON_SOURCE: &str = r#"
from typing import Optional, List
from dataclasses import dataclass
from datetime import datetime

@dataclass
class User:
    id: str
    name: str
    email: str
    created_at: datetime

class UserRepository:
    def __init__(self, db_connection):
        self.db = db_connection

    def find_by_id(self, user_id: str) -> Optional[User]:
        row = self.db.execute("SELECT * FROM users WHERE id = ?", (user_id,))
        if row is None:
            return None
        return User(**row)

    def find_all(self) -> List[User]:
        rows = self.db.execute("SELECT * FROM users")
        return [User(**row) for row in rows]

    def create(self, name: str, email: str) -> User:
        user_id = generate_id()
        now = datetime.utcnow()
        self.db.execute(
            "INSERT INTO users (id, name, email, created_at) VALUES (?, ?, ?, ?)",
            (user_id, name, email, now),
        )
        return User(id=user_id, name=name, email=email, created_at=now)

    def delete(self, user_id: str) -> bool:
        result = self.db.execute("DELETE FROM users WHERE id = ?", (user_id,))
        return result.rowcount > 0

def generate_id() -> str:
    import uuid
    return str(uuid.uuid4())
"#;

fn bench_parse_typescript(c: &mut Criterion) {
    c.bench_function("parse_typescript_file", |b| {
        b.iter(|| {
            let mut parser = TreeSitterParser::new();
            parser
                .parse_file(
                    "typescript",
                    Path::new("src/controllers/user.ts"),
                    black_box(TYPESCRIPT_SOURCE),
                )
                .unwrap();
        })
    });
}

fn bench_parse_python(c: &mut Criterion) {
    c.bench_function("parse_python_file", |b| {
        b.iter(|| {
            let mut parser = TreeSitterParser::new();
            parser
                .parse_file(
                    "python",
                    Path::new("src/repositories/user.py"),
                    black_box(PYTHON_SOURCE),
                )
                .unwrap();
        })
    });
}

// ---------------------------------------------------------------------------
// SQLite store benchmarks
// ---------------------------------------------------------------------------

fn make_test_node(id: u64, name: &str, module_id: u64) -> GraphNode {
    GraphNode {
        id,
        hash: format!("bench_hash_{:05}", id),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: format!("fn {}(x: i32) -> i32", name),
        file_path: format!("src/mod_{}.rs", module_id),
        line_start: (id * 10) as u32,
        line_end: (id * 10 + 8) as u32,
        docstring: Some(format!("Documentation for {}", name)),
        is_public: true,
        type_hints_present: true,
        has_docstring: true,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id,
        package: None,
    }
}

fn bench_sqlite_insert_nodes(c: &mut Criterion) {
    c.bench_function("sqlite_insert_100_nodes", |b| {
        b.iter(|| {
            let mut store =
                SqliteGraphStore::in_memory().expect("Failed to create in-memory store");
            let changes: Vec<NodeChange> = (1..=100)
                .map(|i| NodeChange::Add(make_test_node(i, &format!("func_{}", i), 0)))
                .collect();
            store.update_nodes(changes).unwrap();
        })
    });
}

fn bench_sqlite_lookup_by_hash(c: &mut Criterion) {
    let mut store = SqliteGraphStore::in_memory().expect("Failed to create in-memory store");
    let changes: Vec<NodeChange> = (1..=500)
        .map(|i| NodeChange::Add(make_test_node(i, &format!("func_{}", i), 0)))
        .collect();
    store.update_nodes(changes).unwrap();

    c.bench_function("sqlite_lookup_by_hash", |b| {
        b.iter(|| {
            store.get_node(black_box("bench_hash_00250")).unwrap();
        })
    });
}

fn bench_sqlite_get_edges(c: &mut Criterion) {
    let mut store = SqliteGraphStore::in_memory().expect("Failed to create in-memory store");

    // Insert nodes
    let nodes: Vec<NodeChange> = (1..=100)
        .map(|i| NodeChange::Add(make_test_node(i, &format!("func_{}", i), 0)))
        .collect();
    store.update_nodes(nodes).unwrap();

    // Insert edges: node 1 calls nodes 2-50
    let edges: Vec<EdgeChange> = (2..=50)
        .map(|i| {
            EdgeChange::Add(GraphEdge {
                id: i as u64,
                source_id: 1,
                target_id: i as u64,
                kind: EdgeKind::Calls,
                file_path: "src/main.rs".to_string(),
                line: i as u32,
                confidence: 1.0,
            })
        })
        .collect();
    store.update_edges(edges).unwrap();

    c.bench_function("sqlite_get_outgoing_edges_50", |b| {
        b.iter(|| {
            store.get_edges(
                black_box(1),
                keel_core::types::EdgeDirection::Outgoing,
            );
        })
    });
}

fn bench_sqlite_get_nodes_in_file(c: &mut Criterion) {
    let mut store = SqliteGraphStore::in_memory().expect("Failed to create in-memory store");

    // Insert 200 nodes across 10 files (20 per file)
    let nodes: Vec<NodeChange> = (1..=200)
        .map(|i| {
            let file_idx = (i - 1) / 20;
            let mut node = make_test_node(i, &format!("func_{}", i), 0);
            node.file_path = format!("src/module_{}.rs", file_idx);
            NodeChange::Add(node)
        })
        .collect();
    store.update_nodes(nodes).unwrap();

    c.bench_function("sqlite_get_nodes_in_file_20", |b| {
        b.iter(|| {
            store.get_nodes_in_file(black_box("src/module_5.rs"));
        })
    });
}

criterion_group!(
    benches,
    bench_hash_computation,
    bench_parse_typescript,
    bench_parse_python,
    bench_sqlite_insert_nodes,
    bench_sqlite_lookup_by_hash,
    bench_sqlite_get_edges,
    bench_sqlite_get_nodes_in_file,
);
criterion_main!(benches);
