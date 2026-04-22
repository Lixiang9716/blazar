use super::{ScheduledCall, schedule_batches};
use crate::agent::tools::{ResourceAccess, ResourceClaim};

fn call(id: &'static str, claims: Vec<ResourceClaim>) -> ScheduledCall<&'static str> {
    ScheduledCall { item: id, claims }
}

#[test]
fn scheduler_batches_shared_read_only_claims_together() {
    let batches = schedule_batches(vec![
        call(
            "read-a",
            vec![ResourceClaim {
                resource: "fs:src/lib.rs".into(),
                access: ResourceAccess::ReadOnly,
            }],
        ),
        call(
            "read-b",
            vec![ResourceClaim {
                resource: "fs:src/lib.rs".into(),
                access: ResourceAccess::ReadOnly,
            }],
        ),
    ]);

    assert_eq!(batches.len(), 1);
    assert_eq!(
        batches[0].iter().map(|call| call.item).collect::<Vec<_>>(),
        vec!["read-a", "read-b"]
    );
}

#[test]
fn scheduler_serializes_read_write_conflicts_on_same_resource() {
    let batches = schedule_batches(vec![
        call(
            "read-a",
            vec![ResourceClaim {
                resource: "fs:src/lib.rs".into(),
                access: ResourceAccess::ReadOnly,
            }],
        ),
        call(
            "write-a",
            vec![ResourceClaim {
                resource: "fs:src/lib.rs".into(),
                access: ResourceAccess::ReadWrite,
            }],
        ),
        call(
            "read-b",
            vec![ResourceClaim {
                resource: "fs:src/other.rs".into(),
                access: ResourceAccess::ReadOnly,
            }],
        ),
    ]);

    assert_eq!(batches.len(), 2);
    assert_eq!(
        batches[0].iter().map(|call| call.item).collect::<Vec<_>>(),
        vec!["read-a"]
    );
    assert_eq!(
        batches[1].iter().map(|call| call.item).collect::<Vec<_>>(),
        vec!["write-a", "read-b"]
    );
}

#[test]
fn scheduler_treats_exclusive_claims_as_global_conflicts() {
    let batches = schedule_batches(vec![
        call(
            "read-a",
            vec![ResourceClaim {
                resource: "fs:src/lib.rs".into(),
                access: ResourceAccess::ReadOnly,
            }],
        ),
        call(
            "bash",
            vec![ResourceClaim {
                resource: "process:bash".into(),
                access: ResourceAccess::Exclusive,
            }],
        ),
        call(
            "read-b",
            vec![ResourceClaim {
                resource: "fs:src/other.rs".into(),
                access: ResourceAccess::ReadOnly,
            }],
        ),
    ]);

    assert_eq!(batches.len(), 3);
    assert_eq!(
        batches[0].iter().map(|call| call.item).collect::<Vec<_>>(),
        vec!["read-a"]
    );
    assert_eq!(
        batches[1].iter().map(|call| call.item).collect::<Vec<_>>(),
        vec!["bash"]
    );
    assert_eq!(
        batches[2].iter().map(|call| call.item).collect::<Vec<_>>(),
        vec!["read-b"]
    );
}
