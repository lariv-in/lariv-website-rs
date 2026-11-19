//! Deployment-only dashboard launchpad: new-tab app tiles, no apps-grid icon.
//!
//! Replaces the shared dashboard topbar apps button with empty markup and owns
//! `GET /dashboard` so tiles open with `target="_blank"`. Install after
//! [`lariv_plugin_dashboard`].

pub mod handlers;
pub mod routes;
pub mod templates;

use frunk::HCons;
use lariv_core::app::App;
use lariv_core::components::{SlotCap, SlotTag};
use lariv_core::http::{HttpCap, HttpTag};
use lariv_core::tag::Tagged;
use lariv_core::traits::get::GetByCapTag;
use lariv_core::traits::replace::MapByCapTag;

/// Capability tag for this deployment's dashboard launch overrides.
pub struct DashboardLaunchTag;

type SlotsHookTyped<AppsBtnIdx> = templates::SlotsHook<AppsBtnIdx>;

type AfterSlots<L, SlotIdx, SlotHooks, SlotItems, AppsBtnIdx> = <L as MapByCapTag<
    SlotTag,
    SlotCap<HCons<Tagged<DashboardLaunchTag, SlotsHookTyped<AppsBtnIdx>>, SlotHooks>, SlotItems>,
    SlotIdx,
>>::Output;

type AfterHttp<L, SlotIdx, SlotHooks, SlotItems, AppsBtnIdx, HttpIdx, HttpHooks, HttpRoutes> =
    <AfterSlots<L, SlotIdx, SlotHooks, SlotItems, AppsBtnIdx> as MapByCapTag<
        HttpTag,
        HttpCap<HCons<Tagged<DashboardLaunchTag, routes::Hook>, HttpHooks>, HttpRoutes>,
        HttpIdx,
    >>::Output;

/// Post-install capability stack for this plugin.
pub type InstallOutput<L, SlotIdx, SlotHooks, SlotItems, AppsBtnIdx, HttpIdx, HttpHooks, HttpRoutes> =
    App<AfterHttp<L, SlotIdx, SlotHooks, SlotItems, AppsBtnIdx, HttpIdx, HttpHooks, HttpRoutes>>;

/// Hide the apps-grid topbar button and serve a new-tab launchpad at `/dashboard`.
#[allow(
    clippy::type_complexity,
    reason = "capability HList indices for slot replace + route override"
)]
pub fn install<L, SlotIdx, SlotHooks, SlotItems, AppsBtnIdx, HttpIdx, HttpHooks, HttpRoutes>(
    app: App<L>,
) -> InstallOutput<L, SlotIdx, SlotHooks, SlotItems, AppsBtnIdx, HttpIdx, HttpHooks, HttpRoutes>
where
    L: GetByCapTag<SlotTag, SlotIdx, Value = SlotCap<SlotHooks, SlotItems>>,
    L: MapByCapTag<
            SlotTag,
            SlotCap<
                HCons<Tagged<DashboardLaunchTag, SlotsHookTyped<AppsBtnIdx>>, SlotHooks>,
                SlotItems,
            >,
            SlotIdx,
            OldValue = SlotCap<SlotHooks, SlotItems>,
        >,
    AfterSlots<L, SlotIdx, SlotHooks, SlotItems, AppsBtnIdx>:
        GetByCapTag<HttpTag, HttpIdx, Value = HttpCap<HttpHooks, HttpRoutes>>,
    AfterSlots<L, SlotIdx, SlotHooks, SlotItems, AppsBtnIdx>: MapByCapTag<
            HttpTag,
            HttpCap<HCons<Tagged<DashboardLaunchTag, routes::Hook>, HttpHooks>, HttpRoutes>,
            HttpIdx,
            OldValue = HttpCap<HttpHooks, HttpRoutes>,
        >,
{
    app.replace_capability::<SlotTag, SlotIdx, _>(|cap: SlotCap<SlotHooks, SlotItems>| {
        cap.add_hook(SlotsHookTyped::<AppsBtnIdx>::default())
    })
    .replace_capability::<HttpTag, HttpIdx, _>(|cap: HttpCap<HttpHooks, HttpRoutes>| {
        cap.add_hook(routes::Hook::default())
    })
}
