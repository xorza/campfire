use bevy_ecs::system::{Local, Query, Res};
use campfire_console::{Reply, Request};
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

    /// Whether the server is a client's local one, which takes its client's console.
    pub(crate) fn is_local(hosting: Res<'_, Hosting>) -> bool {
        *hosting == Hosting::Local
    }
}
