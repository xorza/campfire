use std::fmt::Write as _;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::camera::visibility::Visibility;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Allow, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut, Single};
use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use bevy::text::TextColor;
use bevy::ui::widget::Text;
use campfire_capabilities::Order;
use campfire_items::{Inventory, ItemsDirective, Purchase, Shopping};
use campfire_net::PendingOrders;
use campfire_sim::{StableId, Unpredicted};
use campfire_values::ItemId;

use crate::input_map::{Act, InputMap};
use crate::item_names::ItemTexts;
use crate::own_player::OwnPlayer;
use crate::ui_style::{self, Corner, DIMMED, Panel, PanelRow, TEXT};
use crate::view::ViewSystems;

/// The shop panel, as League of Legends': the shop chord, P of the engine's, opens and closes
/// it; it shows the nearest living shop unit whose buyers select the own avatar, its items in
/// their order, each
/// with its name and its price for the avatar now, and the player's amount of the shop's
/// resource above; an item the avatar cannot buy now is dimmed, and a click buys it.
#[derive(Debug)]
pub(crate) struct ShopPanelPlugin;

/// The panel's root, shown while it is open.
#[derive(Component, Debug)]
struct PanelRoot;

/// The panel's line of the player's amount of the shop's resource.
#[derive(Component, Debug)]
struct Funds;

/// The holder of the panel's rows.
#[derive(Component, Debug)]
struct Rows;

/// A row of the panel: the item it sells, and its lines of name and price.
#[derive(Component, Debug)]
struct ShopRow {
    item: ItemId,
    name: Entity,
    price: Entity,
}

/// The shop unit the panel shows, whose rows it holds; none before it shows one.
#[derive(Resource, Debug, Default)]
struct Shown(Option<StableId>);

impl Plugin for ShopPanelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shown>();
        app.add_systems(Startup, Self::spawn);
        app.add_systems(
            Update,
            (Self::toggle.run_if(InputMap::on(Act::Shop)), Self::show)
                .chain()
                .after(ViewSystems),
        );
    }
}

impl ShopPanelPlugin {
    fn spawn(mut commands: Commands<'_, '_>) {
        let panel = Panel::spawn(&mut commands, Corner::TopRight, 280.0);
        commands.entity(panel.root).insert(PanelRoot);
        commands.entity(panel.funds).insert(Funds);
        commands.entity(panel.rows).insert(Rows);
    }

    fn toggle(mut panel: Single<'_, '_, &mut Visibility, With<PanelRoot>>) {
        let shown = **panel == Visibility::Hidden;
        **panel = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    /// Shows the shop the panel reads for the own avatar, while it is open: its rows, made again
    /// when it shows another shop, and each row's price and whether it is dimmed, with the
    /// player's amount above.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    fn show(
        panel: Single<'_, '_, &Visibility, With<PanelRoot>>,
        own: OwnPlayer<'_, '_>,
        shopping: Shopping<'_, '_>,
        carriers: Query<'_, '_, &Inventory, Allow<Unpredicted>>,
        (mut shown, names): (ResMut<'_, Shown>, Res<'_, ItemTexts>),
        (rows, mut texts): (Single<'_, '_, Entity, With<Rows>>, Query<'_, '_, &mut Text>),
        (funds, mut colors, row_items): (
            Single<'_, '_, Entity, With<Funds>>,
            Query<'_, '_, &mut TextColor>,
            Query<'_, '_, &ShopRow>,
        ),
        mut commands: Commands<'_, '_>,
        (mut line, mut given_up): (Local<'_, String>, Local<'_, Vec<u32>>),
    ) {
        if **panel == Visibility::Hidden {
            return;
        }
        let avatar = own.avatar();
        let shop = avatar.and_then(|avatar| shopping.nearest_for(avatar.id));
        if shown.0 != shop {
            shown.0 = shop;
            commands.entity(*rows).despawn_related::<Children>();
            let sold = shop.and_then(|shop| shopping.shop(shop));
            // A row of each item, which a click buys one of at the shop the panel shows.
            for &item in sold.map_or(&[][..], |sold| sold.items()) {
                let PanelRow { row, name, value } =
                    PanelRow::spawn(&mut commands, *rows, names.of(item));
                commands
                    .entity(row)
                    .insert(ShopRow {
                        item,
                        name,
                        price: value,
                    })
                    .observe(Self::buy);
            }
            return;
        }
        let (Some(avatar), Some(shop)) = (avatar, shop) else {
            return;
        };
        let (Some(sold), Ok(inventory)) = (shopping.shop(shop), carriers.get(avatar.entity)) else {
            return;
        };
        let service = if shopping.serving(shop, avatar.id).is_some() {
            Service::Serves
        } else {
            Service::Refuses
        };
        let amount = own.amount(sold.resource());
        line.clear();
        write!(line, "{amount}").expect("a String takes any text");
        ui_style::set_text(&mut texts, *funds, &line);
        for row in &row_items {
            let purchase = shopping.purchase(sold, inventory, row.item, &mut given_up);
            line.clear();
            if let Some(Purchase { price, .. }) = purchase {
                write!(line, "{price}").expect("a String takes any text");
            }
            ui_style::set_text(&mut texts, row.price, &line);
            let color = if is_dimmed(service, purchase, amount) {
                DIMMED
            } else {
                TEXT
            };
            for line in [row.name, row.price] {
                if let Ok(mut held) = colors.get_mut(line) {
                    held.set_if_neq(TextColor(color));
                }
            }
        }
    }

    /// A left click on a row buys its item at the shop the panel shows.
    fn buy(
        click: On<'_, '_, PointerClick>,
        rows: Query<'_, '_, &ShopRow>,
        shown: Res<'_, Shown>,
        own: OwnPlayer<'_, '_>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        if click.button != PointerButton::Primary {
            return;
        }
        let (Ok(row), Some(shop), Some(avatar)) = (rows.get(click.entity), shown.0, own.avatar())
        else {
            return;
        };
        let action = ItemsDirective::Buy {
            item: row.item,
            shop,
        };
        orders.push(Order::one(avatar.id, action));
    }
}

/// Whether a shop serves the avatar now; it refuses a living one out of its reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Service {
    Serves,
    Refuses,
}

/// Whether the panel dims an item: the shop refuses the avatar now; or the shop does not sell
/// it, or the avatar cannot pay its price now, or has no room for it.
fn is_dimmed(service: Service, purchase: Option<Purchase>, amount: i64) -> bool {
    service == Service::Refuses
        || purchase.is_none_or(|purchase| !purchase.room || amount < purchase.price)
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::ecs::world::World;

    use super::*;

    #[test]
    fn p_shows_the_panel_and_p_again_hides_it() {
        // The panel is hidden as it spawns.
        let mut world = World::new();
        world.run_system_once(ShopPanelPlugin::spawn).unwrap();
        let shown = |world: &mut World| {
            let mut panel = world.query_filtered::<&Visibility, With<PanelRoot>>();
            *panel.single(world).unwrap() != Visibility::Hidden
        };
        assert!(!shown(&mut world));
        for open in [true, false] {
            world.run_system_once(ShopPanelPlugin::toggle).unwrap();
            assert_eq!(shown(&mut world), open);
        }
    }

    #[test]
    fn an_item_is_dimmed_out_of_a_shop_too_dear_or_with_no_room() {
        let buys = |price, room| Some(Purchase { price, room });
        // Served, with 300 to spend: an item of 300 with room is bright.
        assert!(!is_dimmed(Service::Serves, buys(300, true), 300));
        // A living avatar out of the shop's reach; too poor by one; no room; one it does not
        // sell.
        assert!(is_dimmed(Service::Refuses, buys(300, true), 300));
        assert!(is_dimmed(Service::Serves, buys(301, true), 300));
        assert!(is_dimmed(Service::Serves, buys(50, false), 300));
        assert!(is_dimmed(Service::Serves, None, 300));
    }
}
