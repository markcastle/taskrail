//! The first thing a new user sees: three cards that teach the gestures.
//! They are ordinary local tasks, so flicking them away is the tutorial.
//! Once a real connector is added the lesson is over, and any that are
//! still lying around are taken off the rail.

use taskrail_core::ports::{Clock, ItemRepository};
use taskrail_core::services::{RailService, ServiceError};

const CURRENT: [&str; 3] = [
    "Drag a card up or down to reorder it",
    "Pull a card into In Progress to start it",
    "Flick a card sideways to complete it",
];

/// Wording from earlier builds, so their cards are recognised too.
const EARLIER: [&str; 1] = ["Press and hold to drag, then drop to reorder"];

pub fn welcome<R: ItemRepository, C: Clock>(
    service: &mut RailService<R, C>,
) -> Result<(), ServiceError> {
    // Added in reverse because each quick-add lands at the top.
    for title in CURRENT.iter().rev() {
        service.add_task(title)?;
    }
    Ok(())
}

/// Remove every welcome card that is still on the rail. Returns how many went.
pub fn retire<R: ItemRepository, C: Clock>(
    service: &mut RailService<R, C>,
) -> Result<usize, ServiceError> {
    let is_welcome = |title: &str| CURRENT.contains(&title) || EARLIER.contains(&title);
    let leftovers: Vec<_> = service
        .items()?
        .into_iter()
        .filter(|item| item.origin.is_none() && is_welcome(&item.title))
        .map(|item| item.id)
        .collect();
    for id in &leftovers {
        service.remove(id)?;
    }
    Ok(leftovers.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskrail_core::memory::InMemoryItemRepository;
    use taskrail_core::ports::SystemClock;

    #[test]
    fn welcome_cards_go_when_asked_but_real_tasks_stay() {
        let mut service = RailService::new(InMemoryItemRepository::default(), SystemClock);
        welcome(&mut service).unwrap();
        service
            .add_task("Press and hold to drag, then drop to reorder")
            .unwrap();
        service.add_task("Call the bank").unwrap();
        assert_eq!(service.items().unwrap().len(), 5);
        assert_eq!(retire(&mut service).unwrap(), 4);
        let left = service.items().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].title, "Call the bank");
        assert_eq!(retire(&mut service).unwrap(), 0, "nothing left to retire");
    }
}
