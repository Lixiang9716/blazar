use crate::agent::tools::{ResourceAccess, ResourceClaim};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ScheduledCall<T> {
    pub(crate) item: T,
    pub(crate) claims: Vec<ResourceClaim>,
}

pub(crate) fn schedule_batches<T>(calls: Vec<ScheduledCall<T>>) -> Vec<Vec<ScheduledCall<T>>> {
    let mut batches = Vec::new();
    let mut current_batch = Vec::new();

    for call in calls {
        if current_batch.is_empty() || !batch_conflicts(&current_batch, &call.claims) {
            current_batch.push(call);
        } else {
            batches.push(current_batch);
            current_batch = vec![call];
        }
    }

    if !current_batch.is_empty() {
        batches.push(current_batch);
    }

    batches
}

fn batch_conflicts<T>(batch: &[ScheduledCall<T>], claims: &[ResourceClaim]) -> bool {
    if batch.is_empty() {
        return false;
    }

    if has_exclusive_claim(claims)
        || batch
            .iter()
            .any(|scheduled_call| has_exclusive_claim(&scheduled_call.claims))
    {
        return true;
    }

    batch
        .iter()
        .any(|scheduled_call| claims_conflict(&scheduled_call.claims, claims))
}

fn has_exclusive_claim(claims: &[ResourceClaim]) -> bool {
    claims
        .iter()
        .any(|claim| matches!(claim.access, ResourceAccess::Exclusive))
}

fn claims_conflict(left: &[ResourceClaim], right: &[ResourceClaim]) -> bool {
    left.iter().any(|left_claim| {
        right.iter().any(|right_claim| {
            left_claim.resource == right_claim.resource
                && !matches!(
                    (left_claim.access, right_claim.access),
                    (ResourceAccess::ReadOnly, ResourceAccess::ReadOnly)
                )
        })
    })
}

#[cfg(test)]
#[path = "scheduler/tests.rs"]
mod tests;
