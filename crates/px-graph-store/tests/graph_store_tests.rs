use px_graph_store::{EdgeKind, EdgeRecord, GraphNode, GraphStore, NodeKey};
use px_repo_model::identity::{ContentHash, ProcedureId, RenderingProfileHash};
use px_repo_model::schema::{
    procedure_revision_hash, revision_id, Procedure, ProcedureKind, ProcedureRevisionContent,
    RevisionContent,
};

fn procedure(name: &str) -> (Procedure, ProcedureRevisionContent) {
    let id = ProcedureId::new();
    let content = ProcedureRevisionContent {
        procedure_id: id,
        source_text: format!("procedure {name}:\n  trigger: manual\n"),
        capability_grants: vec![],
        residuals: vec![],
    };
    let revision = procedure_revision_hash(&content).expect("valid canonical procedure content");
    (
        Procedure {
            id,
            kind: ProcedureKind::State,
            name: name.to_string(),
            path: format!("praxis/procedures/{name}.px"),
            current_revision: revision,
        },
        content,
    )
}

fn revision(
    parents: Vec<px_repo_model::identity::RevisionId>,
    root: ContentHash,
) -> RevisionContent {
    RevisionContent {
        parents,
        procedure_root: root,
        entity_root: ContentHash::of_raw_bytes(b"entities"),
        change_ids: vec![],
        rendering_profile: RenderingProfileHash::of_raw_bytes(b"profile"),
        materialization_root: ContentHash::of_raw_bytes(b"materialization"),
        author: "test".to_string(),
        timestamp: "2026-07-31T00:00:00Z".to_string(),
        message: "test revision".to_string(),
    }
}

#[test]
fn persists_and_reads_a_graph_fixture_through_pluresdb() {
    // A real durable PluresDB/Sled instance is opened twice; the second store
    // proves the node was persisted by PluresDB rather than retained in a
    // process-local collection.
    let directory = tempfile::tempdir().unwrap();
    let (procedure, _content) = procedure("persisted");
    {
        let store = GraphStore::open(directory.path()).unwrap();
        store.put_procedure(procedure.clone()).unwrap();
    }

    let store = GraphStore::open(directory.path()).unwrap();
    let key = NodeKey::procedure(procedure.id);
    assert_eq!(
        store.get_node(&key).unwrap(),
        Some(GraphNode::Procedure(procedure.clone()))
    );
    assert_eq!(store.get_procedure(procedure.id).unwrap(), Some(procedure));
}

#[test]
fn procedure_merkle_root_is_stable_and_independent_of_write_order() {
    let (first, _) = procedure("first");
    let (second, _) = procedure("second");

    let left = GraphStore::in_memory();
    left.put_procedure(first.clone()).unwrap();
    left.put_procedure(second.clone()).unwrap();

    let right = GraphStore::in_memory();
    right.put_procedure(second.clone()).unwrap();
    right.put_procedure(first.clone()).unwrap();

    let root = left.current_procedure_root().unwrap();
    assert_eq!(root, left.current_procedure_root().unwrap());
    assert_eq!(root, right.current_procedure_root().unwrap());
}

#[test]
fn query_children_and_historical_merkle_root_are_pluresdb_backed() {
    let store = GraphStore::in_memory();
    let (parent, _) = procedure("parent");
    let (child, _) = procedure("child");
    store.put_procedure(parent.clone()).unwrap();
    store.put_procedure(child.clone()).unwrap();

    store
        .put_edge(EdgeRecord::new(
            NodeKey::repository("test-repository"),
            EdgeKind::RepositoryContainsProcedure,
            NodeKey::procedure(child.id),
        ))
        .unwrap();
    assert_eq!(
        store
            .get_children(
                &NodeKey::repository("test-repository"),
                EdgeKind::RepositoryContainsProcedure
            )
            .unwrap(),
        vec![NodeKey::procedure(child.id)]
    );

    let frozen_root = ContentHash::of_raw_bytes(b"procedure-root-at-r1");
    let revision = revision(vec![], frozen_root);
    let revision_id = revision_id(&revision).unwrap();
    store.put_revision(revision_id, revision).unwrap();
    assert_eq!(
        store.merkle_root_at_revision(revision_id).unwrap(),
        Some(frozen_root)
    );
}

#[test]
fn revision_persistence_materializes_parent_to_child_edges() {
    let store = GraphStore::in_memory();
    let root = revision(vec![], ContentHash::of_raw_bytes(b"root"));
    let root_id = revision_id(&root).unwrap();
    store.put_revision(root_id, root).unwrap();

    let child = revision(vec![root_id], ContentHash::of_raw_bytes(b"child"));
    let child_id = revision_id(&child).unwrap();
    store.put_revision(child_id, child).unwrap();

    assert_eq!(
        store
            .get_children(&NodeKey::revision(root_id), EdgeKind::RevisionParentOf)
            .unwrap(),
        vec![NodeKey::revision(child_id)]
    );
}
