use bevy_ecs::system::{Local, Query, Res};
use campfire_console::{ConsoleAnswer, ConsoleRefusal, Reply, Request};
use campfire_host::{ConsoleStdio, Hosting, MatchConsole};
use campfire_net_messages::{MessageReceiver, MessageSender};

use crate::net_protocol::MatchChannel;

/// The transports of a server's console: a dedicated server's standard streams, and a local
/// server's link to its client, whose console forwards its requests (02-engine-core.md, A game's
/// console is its standard streams).
#[derive(Debug)]
pub(crate) struct ServerConsole;

impl ServerConsole {
    /// Queues the requests the standard input brought.
    pub(crate) fn take_stdio(
        (stdio, console): (Res<'_, ConsoleStdio>, Res<'_, MatchConsole>),
        mut taken: Local<'_, Vec<Request>>,
    ) {
        stdio.take(&mut taken);
        console.push(taken.drain(..));
    }

    /// Writes the console's replies to the standard output.
    pub(crate) fn send_stdio(
        (stdio, console): (Res<'_, ConsoleStdio>, Res<'_, MatchConsole>),
        mut replies: Local<'_, Vec<Reply>>,
    ) {
        console.take_replies(&mut replies);
        for reply in replies.drain(..) {
            stdio.send(reply);
        }
    }

    /// Queues the requests a local server's client forwarded.
    pub(crate) fn take_links(
        console: Res<'_, MatchConsole>,
        mut links: Query<'_, '_, &mut MessageReceiver<Request>>,
    ) {
        for mut receiver in &mut links {
            console.push(receiver.receive());
        }
    }

    /// Sends the console's replies to a local server's client, the one link it holds.
    pub(crate) fn send_links(
        console: Res<'_, MatchConsole>,
        mut links: Query<'_, '_, &mut MessageSender<Reply>>,
        mut replies: Local<'_, Vec<Reply>>,
    ) {
        console.take_replies(&mut replies);
        for reply in replies.drain(..) {
            for mut sender in &mut links {
                sender.send::<MatchChannel>(reply.clone());
            }
        }
    }

    /// Refuses each request a link brought to a server that is no client's local one, as it
    /// refuses a journal's command, so a remote peer's requests reach no console and no log.
    pub(crate) fn refuse_links(
        mut links: Query<'_, '_, (&mut MessageReceiver<Request>, &mut MessageSender<Reply>)>,
    ) {
        for (mut receiver, mut sender) in &mut links {
            for request in receiver.receive() {
                sender.send::<MatchChannel>(Reply {
                    id: request.id,
                    tick: None,
                    answer: ConsoleAnswer::Refused(ConsoleRefusal::NotLocal),
                });
            }
        }
    }

    /// Whether the server is a client's local one, which takes its client's console.
    pub(crate) fn is_local(hosting: Res<'_, Hosting>) -> bool {
        *hosting == Hosting::Local
    }
}

#[cfg(test)]
mod tests {
    use bevy_app::App;
    use bevy_ecs::system::RunSystemOnce;
    use campfire_console::{ConsoleAsk, RequestId};
    use campfire_net_messages::AppMessageExt;

    use super::*;

    #[test]
    fn a_server_no_client_hosts_refuses_each_linked_request() {
        // Two requests on one link take two refusals to that link, and leave none unread.
        let mut app = App::new();
        app.register_message::<Request>();
        app.register_message::<Reply>();
        let world = app.world_mut();
        let mut receiver = MessageReceiver::<Request>::default();
        for id in [1, 2] {
            receiver.bridge_push::<MatchChannel>(Request {
                id: RequestId(id),
                ask: ConsoleAsk::Status,
            });
        }
        let link = world
            .spawn((receiver, MessageSender::<Reply>::default()))
            .id();
        world.run_system_once(ServerConsole::refuse_links).unwrap();
        let link = world.entity(link);
        assert!(link.get::<MessageReceiver<Request>>().unwrap().is_empty());
        assert_eq!(
            link.get::<MessageSender<Reply>>().unwrap().bridge_queued(),
            2
        );
    }
}
