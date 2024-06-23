pub use matrix_bot_rs_macros::*;
pub use matrix_sdk;
use matrix_sdk::{
    config::SyncSettings, ruma::events::room::message::{RoomMessageEventContent, SyncRoomMessageEvent}, Client, Room
};
use std::{
    future::Future,
    pin::Pin,
    time::SystemTime,
};
pub struct Bot<'a> {
    pub client: matrix_sdk::Client,
    pub commands: &'a [Command<'a>],
    pub command_prefix: String,
}
impl Bot<'static> {
    pub fn new(client: matrix_sdk::Client, command_prefix: String,commands: &'static [Command]) -> Self {
        Self {
            client,
            commands,
            command_prefix,
        }
    }
    pub async fn run(self) -> Result<(), matrix_sdk::Error> {
        // TODO: Make it so there aren't so many clones
        let start_time = SystemTime::now();
        let prefix = self.command_prefix.clone();
        let client_cloned = self.client.clone();
        let commands=self.commands.to_vec();
        self.client.add_event_handler(move|ev,room|handle_message_event(ev,room,start_time.clone(),prefix.clone(),client_cloned.clone(),commands));
        self.client.sync(SyncSettings::new()).await
    }
}
async fn handle_message_event(event:SyncRoomMessageEvent,room: Room,start_time:SystemTime,prefix: String,client:Client,commands: Vec<Command<'_>>){
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
            let mut most_matching: Option<(usize, usize,&str)>=None;
            for (command_i,command) in commands.iter().enumerate(){
                let all_names=[command.aliases,&[command.name]].concat();
                for name in all_names{
                let name_words=name.split_whitespace();
                for (i,(candidate_word,name_word)) in candidate_words.iter().zip(name_words).enumerate(){
                    if *candidate_word!=name_word{
                        break
                    }
                    if let Some(matching)=most_matching{
                        if matching.0>i{
                            most_matching=Some((i,command_i,name))
                        }
                    }else{
                        most_matching=Some((i,command_i,name))
                    }
                   
                }}
            }
            if let Some(index)=most_matching{
                let to_run=&commands[index.1];
                let command_outcome=(to_run.handler)(CallingContext{
                    client: &client,
                },trimmed.strip_prefix(index.2).unwrap_or(trimmed).to_owned()).await;
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
                        let _=room.send(RoomMessageEventContent::text_markdown(format!("# You don't have enough power to run this command\nRequired power level: **{}**",to_run.power_level_required))).await;
                        return
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
}
pub type AsyncHandlerReturn<'a>=Pin<Box<dyn Future<Output = HandlerReturn>+Send+'a>>;
pub type HandlerReturn=Result<(),CommandError>;
pub type CommandHandler<'a>=fn(ctx: CallingContext,args: String)->AsyncHandlerReturn;
pub enum CommandError {
    InternalError(String),
    ArgParseError(String),
}

#[derive(Clone)]
pub struct Command<'a> {
    pub name: &'a str,
    pub aliases: &'a [&'a str],
    pub arg_hints: Vec<CommandArgHint>,
    pub power_level_required: usize,
    pub handler: CommandHandler<'a>,
}
#[derive(Clone)]
pub struct CommandArgHint {
    pub name: String,
    pub description: String
}
pub struct CallingContext<'a> {
    pub client: &'a matrix_sdk::Client,
}

pub trait TryFromStr
where
    Self: Sized,
{
    fn try_from_str(input: &str) -> Result<(Self, &str), String>;
}
impl TryFromStr for String {
    fn try_from_str(input: &str) -> Result<(Self, &str), String> {
        if let Some(index) = input.find(' ') {
            let before_space = &input[..index];
            let after_space = &input[index + 1..];
            return Ok((before_space.to_owned(),after_space))
        } else {
            return Ok((input.to_string(),""));
        }
    }
}

impl<T: TryFromStr> TryFromStr for Option<T>{
    fn try_from_str(input: &str) -> Result<(Self, &str), String> {
        if input.is_empty(){
            return Ok((None,input))
        }
        match T::try_from_str(input){
            Ok(t)=>Ok((Some(t.0),t.1)),
            Err(e)=>Err(e)
        }
    }
}