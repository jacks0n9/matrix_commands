pub use matrix_commands_macros::*;
pub use matrix_sdk;
use matrix_sdk::{
    config::SyncSettings, room::RoomMember, ruma::{
        api::client::message::send_message_event,
        events::{
            room::message::{RoomMessageEventContent, SyncRoomMessageEvent},
            MessageLikeEventContent,
        },
    }, Client, Room
};
use std::{future::Future, pin::Pin, time::SystemTime};
pub struct Bot {
    pub client: matrix_sdk::Client,
    pub commands: Vec<Command>,
    pub command_prefix: String,
}
impl Bot {
    pub async fn run(self) -> Result<(), matrix_sdk::Error> {
        // TODO: Make it so there aren't so many clones
        let start_time = SystemTime::now();
        self.client.add_event_handler(
            move |event: SyncRoomMessageEvent, room: Room, client: Client| async move {
                handle_message_event(
                    event,
                    room,
                    start_time,
                    self.command_prefix,
                    client,
                    self.commands.clone(),
                )
                .await;
            },
        );
        self.client.sync(SyncSettings::default()).await
    }
}
async fn handle_message_event(
    event: SyncRoomMessageEvent,
    room: Room,
    start_time: SystemTime,
    prefix: String,
    client: Client,
    commands: Vec<Command>,
) {
    let og = match event.as_original() {
        Some(og) => og,
        None => return,
    };
    let message_sent_ts =
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(og.origin_server_ts.0.into());
    if message_sent_ts < start_time {
        return;
    }
    let trimmed = match og.content.body().strip_prefix(&prefix) {
        Some(trimmed) => trimmed,
        None => return,
    };
    let candidate_words: Vec<_> = trimmed.split_whitespace().collect();
    let mut most_matching: Option<(usize, usize, String)> = None;
    for (command_i, command) in commands.iter().enumerate() {
        let all_names = [command.aliases.as_slice(), &[command.name.clone()]].concat();
        for name in all_names {
            let name_words = name.split_whitespace();
            for (i, (candidate_word, name_word)) in
                candidate_words.iter().zip(name_words).enumerate()
            {
                if *candidate_word != name_word {
                    break;
                }
                if let Some(ref matching) = most_matching {
                    if matching.0 > i {
                        most_matching =
                            Some((i, command_i, format!("{} {}", matching.2, name.clone())))
                    }
                } else {
                    most_matching = Some((i, command_i, name.clone()))
                }
            }
        }
    }
    let index = match &most_matching {
        Some(index) => index,
        None => return,
    };
    let to_run = &commands[index.1];
    let argument_string = &trimmed.strip_prefix(&index.2).unwrap_or(trimmed).to_owned();
    let member = match room.get_member(event.sender()).await {
        Ok(member_opt) => match member_opt {
            Some(member) => member,
            None => return,
        },
        Err(e) => {
            log::warn!("Failed to get room member: {}", e);
            return;
        }
    };
    if member.power_level() < to_run.power_level_required as i64 {
        let _ = room
            .send(RoomMessageEventContent::text_markdown(format!(
                "# You don't have enough power to run this command\nRequired power level: **{}**",
                to_run.power_level_required
            )))
            .await;
        return;
    }
    let command_outcome = (to_run.handler)(
        CallingContext {
            client: &client,
            room: &room,
            caller: member
        },
        argument_string.clone(),
    )
    .await;
    if let Err(err) = command_outcome {
        let to_send;
        match err {
            CommandError::InternalError(e) => {
                log::error!(
                    "Internal error running command: {}. Command name: {}. Arguments passed: {}",
                    e,
                    to_run.name,
                    argument_string
                );
                to_send = Some(RoomMessageEventContent::text_markdown(format!(
                    "# Internal error\nBot admin has been notified"
                )));
            }
            CommandError::ArgParseError(e) => {
                to_send = Some(RoomMessageEventContent::text_markdown(format!(
                    "Error parsing arguments: {e}"
                )))
            }
        }
        if let Some(content) = to_send {
            if let Err(e) = room.send(content).await {
                log::warn!("Error sending message with content: {e}.");
            }
        }
    }
}
pub type AsyncHandlerReturn<'a> = Pin<Box<dyn Future<Output = HandlerReturn> + Send + 'a>>;
pub type HandlerReturn = Result<(), CommandError>;
pub type CommandHandler = fn(ctx: CallingContext, args: String) -> AsyncHandlerReturn;
pub enum CommandError {
    InternalError(String),
    ArgParseError(String),
}

#[derive(Clone)]
pub struct Command {
    pub name: String,
    pub aliases: Vec<String>,
    pub arg_hints: Vec<CommandArgHint>,
    pub power_level_required: usize,
    pub handler: CommandHandler,
}
#[derive(Clone)]
pub struct CommandArgHint {
    pub name: String,
    pub description: String,
}
pub struct CallingContext<'a> {
    pub client: &'a matrix_sdk::Client,
    pub room: &'a Room,
    pub caller: RoomMember
}

impl CallingContext<'_> {
    pub async fn reply(
        &self,
        content: impl MessageLikeEventContent,
    ) -> Result<send_message_event::v3::Response, CommandError> {
        let serialized = serde_json::to_string_pretty(&content)
            .map_err(|error| CommandError::InternalError(error.to_string()))?;
        self.room.send(content).await.map_err(|error| {
            CommandError::InternalError(format!(
                "Error replying to a message: {}. Message content: {}",
                error, serialized
            ))
        })
    }
}
pub trait TryFromStr
where
    Self: Sized,
{
    fn try_from_str(input: &str) -> Result<(Self, &str), String>;
}
impl TryFromStr for String {
    fn try_from_str(input: &str) -> Result<(Self, &str), String> {
        let input = input.trim();
        if let Some(index) = input.find(' ') {
            let before_space = &input[..index];
            let after_space = &input[index + 1..];
            return Ok((before_space.to_owned(), after_space));
        } else {
            return Ok((input.to_string(), ""));
        }
    }
}

impl<T: TryFromStr> TryFromStr for Option<T> {
    fn try_from_str(input: &str) -> Result<(Self, &str), String> {
        if input.is_empty() {
            return Ok((None, input));
        }
        match T::try_from_str(input) {
            Ok(t) => Ok((Some(t.0), t.1)),
            Err(e) => Err(e),
        }
    }
}
