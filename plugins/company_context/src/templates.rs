//! Shell slots for the company switcher.

use lariv_core::capability::define_register_items;
use lariv_core::components::{SlotCapability, SlotOf, SlotRegistrar, TopbarItemsSlotTag};

pub mod switcher;

pub use switcher::CompaniesSwitcher;

define_register_items! {
    plugin: CompanyContextTag;
    capability: SlotCapability;
    trait: SlotRegistrar;
    method: register_slots;
    wrapper: SlotOf;
    bounds: [];
    hook: SlotsHook;
    items: [
        SwitcherIdx: CompaniesSwitcherTag, TopbarItemsSlotTag => CompaniesSwitcher,
    ]
}
