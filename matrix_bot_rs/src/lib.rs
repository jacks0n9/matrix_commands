pub use matrix_bot_rs_macros::*;
pub use matrix_sdk;
use matrix_sdk::{
    config::SyncSettings,
    ruma::events::room::message::{RoomMessageEventContent, SyncRoomMessageEvent},
    Room,
};
use std::{
    future::Future,
    pin::Pin,
    time::{self, SystemTime},
};
pub struct Bot<'a> {
    client: matrix_sdk::Client,
    commands: &'a [Command<'a>],
    command_prefix: String,
}
impl Bot<'static> {
    pub fn new(client: matrix_sdk::Client, command_prefix: String) -> Self {
        Self {
            client,
            commands: &[],
            command_prefix,
        }
    }
    pub async fn run(self) -> Result<(), matrix_sdk::Error> {
        let start_time = SystemTime::now();
        let prefix = self.command_prefix.clone();
        let client_cloned = self.client.clone();
        self.client.add_event_handler(move|event:SyncRoomMessageEvent,room: Room|async move{
            let og=match event.as_original(){
                Some(og)=>og,
                None=>return
            };
            let message_sent_ts=std::time::UNIX_EPOCH+std::time::Duration::from_millis(og.origin_server_ts.0.into());
            if message_sent_ts<start_time{
                return
            }
            if let Some(trimmed)=og.content.body().strip_prefix(&prefix){
                let candidate_words:Vec<_>=trimmed.split_whitespace().collect();
                let mut most_matching: Option<(usize, usize)>=None;
                for (command_i,command) in self.commands.iter().enumerate(){
                    let name_words=command.name.split_whitespace();
                    for (i,(candidate_word,name_word)) in candidate_words.iter().zip(name_words).enumerate(){
                        if *candidate_word!=name_word{
                            break
                        }
                        if let Some(matching)=most_matching{
                            if matching.0>i{
                                most_matching=Some((i,command_i))
                            }
                        }else{
                            most_matching=Some((i,command_i))
                        }
                       
                    }
                }
                if let Some(index)=most_matching{
                    let to_run=&self.commands[index.1];
                    let command_outcome=(to_run.handler)(CallingContext{
                        client: &client_cloned,
                    },String::from("elefeoefwrw")).await;
                    if let Err(err)=command_outcome{
                        let to_send;
                        let member=match room.get_member(event.sender()).await{
                            Ok(member_opt)=>match member_opt{
                                Some(member)=>member,
                                None=>return
                            }
                            Err(e)=>{
                                eprintln!("{e}");
                                return
                            }
                        };
                        if member.power_level()<to_run.power_level_required as i64{
                            room.send(RoomMessageEventContent::text_markdown(format!("# You don't have enough power to run this command\nRequired power level: **{}**",to_run.power_level_required))).await;
                        }
                        match err{
                            CommandError::InternalError(e)=>{
                                eprintln!("{e}");
                                to_send=Some(RoomMessageEventContent::text_markdown(format!("# Internal error\nBot admin has been notified")));
                            }
                            CommandError::ArgParseError(e)=>{
                                to_send=Some(RoomMessageEventContent::text_markdown(format!("Error parsing arguments: {e}")))
                            }
                        }
                        if let Some(content)=to_send{
                            if let Err(e)=room.send(content).await{
                                eprint!("{e}");
                            }
                        }
                    }
                }else{
                    return
                }
            }
        });
        self.client.sync(SyncSettings::new()).await
    }
}
type AsyncFnPointer<Out> = fn(CallingContext, String) -> Pin<Box<dyn Future<Output = Out> + Send>>;

pub enum CommandError {
    InternalError(String),
    ArgParseError(String),
}

pub struct Command<'a> {
    pub name: &'a str,
    pub aliases: &'a [&'a str],
    pub arg_hints: &'a[CommandArgHint],
    pub power_level_required: usize,
    pub handler: fn(ctx: CallingContext,args: String)->futures::future::BoxFuture<'static,Result<(),CommandError>>,
}
pub struct CommandArgHint {
    pub name: String,
    pub description: Option<String>,
}
pub struct CallingContext<'a> {
    pub client: &'a matrix_sdk::Client,
}
