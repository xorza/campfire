use bevy_app::AppExit;
use bevy_ecs::message::MessageWriter;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Res, ResMut, Single};
use campfire_common::{ExitStatus, Tick};
use campfire_console::{ConsoleAnswer, ConsoleAsk, ConsoleRefusal, Reply, Request};
use campfire_host::{ConsoleStdio, Hosting, JoinState, ReceivedTick};
use campfire_net_connection::Client;
use campfire_net_messages::{MessageReceiver, MessageSender};

use crate::net_protocol::JoinChannel;

/// A client's console: the requests of its view, which its app answers; the requests of the
/// match it forwards to its local server over their link, as its journal commands go, and their
/// replies, a run's held until the client received
/// the state of the tick it paused at and drew a frame of it, so a view after the reply shows
/// that tick (08-sessions.md, A console's local match starts paused). A client of a remote server
/// refuses them. Its transport, the standard streams or a test, fills and drains its queues.
#[derive(Resource, Debug, Default)]
pub struct ClientConsole {
    requests: Vec<Request>,
    /// The requests of the client's view, which its app answers.
    views: Vec<Request>,
    replies: Vec<Reply>,
    held: Vec<HeldRun>,
}

/// A run's reply, held until the client draws the tick it names, and whether a frame of it ran.
#[derive(Debug)]
struct HeldRun {
    reply: Reply,
    drawn: bool,
}

impl ClientConsole {
    /// Queues `requests`, which the next frame forwards in their order.
    pub fn push(&mut self, requests: impl IntoIterator<Item = Request>) {
        self.requests.extend(requests);
    }

    /// Moves the replies made since the last take into `replies`, in their order.
    pub fn take_replies(&mut self, replies: &mut Vec<Reply>) {
        replies.append(&mut self.replies);
    }

    /// Moves the requests of the client's view, a screenshot's or the camera's, into `views`,
    /// for its app to answer.
    pub fn take_views(&mut self, views: &mut Vec<Request>) {
        views.append(&mut self.views);
    }

    /// Gives the reply of a request of the client's view.
    pub fn answer(&mut self, reply: Reply) {
        self.replies.push(reply);
    }

    /// Forwards the queued requests to the local server once the client joined its session; a
    /// client of a remote server refuses each.
    pub(crate) fn forward(
        mut console: ResMut<'_, Self>,
        (hosting, state): (Res<'_, Hosting>, Res<'_, JoinState>),
        mut sender: Single<'_, '_, &mut MessageSender<Request>, With<Client>>,
    ) {
        let console = &mut *console;
        let views = console.requests.extract_if(.., |request| {
            matches!(
                request.ask,
                ConsoleAsk::Screenshot(_) | ConsoleAsk::Camera(_)
            )
        });
        console.views.extend(views);
        if *hosting == Hosting::Remote {
            let refused = console.requests.drain(..).map(|request| Reply {
                id: request.id,
                tick: None,
                answer: ConsoleAnswer::Refused(ConsoleRefusal::NotLocal),
            });
            console.replies.extend(refused);
            return;
        }
        if state.session().is_none() {
            return;
        }
        for request in console.requests.drain(..) {
            sender.send::<JoinChannel>(request);
        }
    }

    /// Takes the server's replies: a run's, held until the client draws its tick.
    pub(crate) fn receive(
        mut console: ResMut<'_, Self>,
        mut receiver: Single<'_, '_, &mut MessageReceiver<Reply>, With<Client>>,
    ) {
        for reply in receiver.receive() {
            if let ConsoleAnswer::Ran(_) = reply.answer {
                console.held.push(HeldRun {
                    reply,
                    drawn: false,
                });
            } else {
                console.replies.push(reply);
            }
        }
    }

    /// Gives each held run's reply in the frame after the first that received the state of its
    /// tick, so a frame of that tick drew before.
    pub(crate) fn release(mut console: ResMut<'_, Self>, received: Res<'_, ReceivedTick>) {
        let console = &mut *console;
        let received = received.get().map(Tick::get);
        let mut at = 0;
        while let Some(held) = console.held.get_mut(at) {
            if held.drawn {
                let held = console.held.remove(at);
                console.replies.push(held.reply);
                continue;
            }
            held.drawn = match held.reply.tick {
                Some(tick) => received.is_some_and(|next| next > tick),
                None => true,
            };
            at += 1;
        }
    }

    /// Queues the requests the standard input brought.
    pub(crate) fn take_stdio(mut console: ResMut<'_, Self>, stdio: Res<'_, ConsoleStdio>) {
        stdio.take(&mut console.requests);
    }

    /// Writes the replies to the standard output; the input's end stops the client, as a stop's
    /// signal does (02-engine-core.md, A game stops as its console's input ends).
    pub(crate) fn send_stdio(
        mut console: ResMut<'_, Self>,
        stdio: Res<'_, ConsoleStdio>,
        mut exit: MessageWriter<'_, AppExit>,
    ) {
        for reply in console.replies.drain(..) {
            stdio.send(reply);
        }
        if stdio.is_ended() {
            exit.write(AppExit::from_code(ExitStatus::Interrupted.code()));
        }
    }
}
