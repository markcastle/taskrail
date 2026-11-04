//! The first thing a new user sees: three cards that teach the gestures.
//! They are ordinary local tasks, so flicking them away is the tutorial.

use taskrail_core::ports::{Clock, ItemRepository};
use taskrail_core::services::{RailService, ServiceError};

pub fn welcome<R: ItemRepository, C: Clock>(
    service: &mut RailService<R, C>,
) -> Result<(), ServiceError> {
    // Added in reverse because each quick-add lands at the top.
    service.add_task("Flick a card sideways to complete it")?;
    service.add_task("Pull a card into In Progress to start it")?;
    service.add_task("Drag a card up or down to reorder it")?;
    Ok(())
}
