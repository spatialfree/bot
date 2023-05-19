use std::collections::HashMap;
use anyhow::anyhow;
use serenity::async_trait;
use serenity::client;
use serenity::model::channel::Message;
use serenity::model::channel::GuildChannel;
use serenity::model::gateway::Ready;
use serenity::model::id::ChannelId;
use serenity::model::id::UserId;
// use serenity::model::prelude::ThreadListSyncEvent;
// serenity::model::channel::ThreadsData;
use serenity::prelude::*;
// use shuttle_service::Environment;
use shuttle_secrets::SecretStore;
use tracing::{error, info};

use lazy_static::lazy_static;

lazy_static! {
	static ref CHANNEL_PROMPTS: HashMap<ChannelId, String> = {
		let mut map = HashMap::new();
		// map.insert("default".to_string(), "You are a helpful assistant.".to_string());
		map.insert(ChannelId(1107008959388332043), "🌟 I want you to act similarly to StumbleUpon 🕵️‍♂️ and randomly suggest something really interesting 🤔 and worth exploring further🔍. Let's create a truly engaging experience for fun! 🤩💡🚀".to_string());
		map.insert(ChannelId(1109159447516946456), "For fun, list five seemingly impossible things, then suggest and briefly explain two potential solutions or approaches for each of the five things using available technologies, (including AI and XR tech), Then offer to do five more".to_string());

		map
	};
}


use async_openai::{
	types::{ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs, Role},
};

struct Bot;


#[async_trait]
impl EventHandler for Bot {
	// async fn thread_create(&self, ctx: Context, channel: GuildChannel) {
	// 	info!("thread: {:?}", channel);
	// 	// unsafe {
	// 	// 	let parent_id : ChannelId = std::env::var("CATEGORY_ID")
	// 	// 		.expect("Expected a parent id in the environment")
	// 	// 		.parse()
	// 	// 		.expect("The parent id was not a valid id");
	// 	// 	if channel.parent_id == Some(parent_id) && !THREAD_IDS.contains(&channel.id.to_string()) {
	// 	// 		THREAD_IDS += &channel.id.to_string();
	// 	// 		// info!("threads: {:?}", THREAD_IDS);
	// 	// 		if let Err(e) = channel.say(&ctx.http, "Hello!").await {
	// 	// 			error!("Error sending message: {:?}", e);
	// 	// 		}
	// 	// 	}
	// 	// }
	// }

	async fn message(&self, ctx: Context, msg: Message) {
		// stop it from looping back in on itself
		if msg.is_own(&ctx.cache) { 
			return;
		}
		
		// check if category is correct
		// let category_id : ChannelId = std::env::var("CATEGORY_ID")
		// 	.expect("Expected a category id in the environment")
		// 	.parse()
		// 	.expect("The category id was not a valid id");
		// if category_id != msg.category_id(&ctx.cache).unwrap_or_else(|| ChannelId(0)) {
		// 	return;
		// }
		// that doesn't work for some inexplicable reason...
		
		let mut in_chat_thread = false;
		let mut system_prompt = "You are a helpful assistant.";
		let threads = msg.guild(&ctx.cache).unwrap().threads;
		threads.iter().filter(|&thread|
			// if inside a matching thread
			msg.channel_id == thread.id
		).for_each(|thread| {
			// check hash map for prompt, if found then replace prompt
			if !in_chat_thread {
				if let Some(p) = CHANNEL_PROMPTS.get(&thread.parent_id.unwrap()) {
					system_prompt = p;
	
					in_chat_thread = true;
				}
			}
		});
		
		if !in_chat_thread {
			return;
		}


		// show typing
		if let Err(e) = msg.channel_id.broadcast_typing(&ctx.http).await {
			error!("Error sending message: {:?}", e);
		}
		
		// info!("msg received from: {:?}", msg.author.name);

		// read all the messages in the thread (usually less than 50)
		let msg_history = msg.channel_id.messages(&ctx.http, |retriever| {
			retriever.limit(100)
		}).await.unwrap();
		// info!("msg_history: {:?}", msg_history);

		// system Role prompt at top of msgs
		// and then msg_history (+new msg)
		let mut msgs = msg_history.iter().map(|msg| {
			ChatCompletionRequestMessageArgs::default()
				.role(message_role(msg.author.id))
				.content(msg.content.clone())
				.build()
				.unwrap()
		}).collect::<Vec<_>>();
		// flip msgs so that oldest is first
		msgs.reverse();
		msgs.insert(0, ChatCompletionRequestMessageArgs::default()
			.role(Role::System)
			.content(system_prompt.to_string())
			.build()
			.unwrap());

		// info!("msgs: {:?}", msgs);
		

		// chat call
		let openai_client = async_openai::Client::new();
		let request = CreateChatCompletionRequestArgs::default()
			.max_tokens(1024u16) // bad default?
			.model("gpt-3.5-turbo")
			.messages(msgs)
			.build()
			.unwrap();
	
		let response = openai_client.chat().create(request).await.unwrap();
		// info!("response: {:?}", response);

		let last_message = response
			.choices
			.last()
			.map(|choice| choice.message.content.clone())
			.unwrap_or_else(String::new);


		let chunks = last_message.chars().collect::<Vec<_>>().chunks(2000).map(|chunk| chunk.iter().collect::<String>()).collect::<Vec<_>>();

    // Send each chunk of the message as a separate message
    for chunk in chunks {
			msg.channel_id.say(&ctx.http, chunk).await.unwrap();
    }

		// alright this works
		// though i want to chop them by 1000 instead of 2000
		// and i want to stream in the chat completion
		// so i can send the first message sooner
		// but that is just polish

		// the main thing is we need to be able to plug in a big prompt
		// and customize the model
		// to reduce the system prompt overhead

	}

	async fn ready(&self, _: Context, ready: Ready) {
		info!("{} is connected!", ready.user.name);
	}
}

fn message_role(user_id: UserId) -> Role {
	let bot_id : UserId = std::env::var("BOT_ID")
		.expect("Expected a bot id in the environment")
		.parse()
		.expect("The bot id was not a valid id");

	if user_id == bot_id {
		Role::System
	} else {
		Role::User
	}
}

#[shuttle_runtime::main]
async fn serenity(
	#[shuttle_secrets::Secrets] secret_store: SecretStore,
) -> shuttle_serenity::ShuttleSerenity {
	// Get the bot id set in `Secrets.toml`
	let bot_id = if let Some(bot_id) = secret_store.get("BOT_ID") {
		bot_id} else {
		return Err(anyhow!("'BOT_ID' was not found").into());
	};
	std::env::set_var("BOT_ID", bot_id);

	// Get the category id set in `Secrets.toml`
	let category_id = if let Some(category_id) = secret_store.get("CATEGORY_ID") { 
		category_id } else {
		return Err(anyhow!("'CATEGORY_ID' was not found").into());
	};
	std::env::set_var("CATEGORY_ID", category_id);

	// Get the openai apikey set in `Secrets.toml`
	let api_key = if let Some(api_key) = secret_store.get("OPENAI_API_KEY") { api_key } else {
		return Err(anyhow!("'OPENAI_API_KEY' was not found").into());
	};
	std::env::set_var("OPENAI_API_KEY", api_key);

	// Get the discord token set in `Secrets.toml`
	let token = if let Some(token) = secret_store.get("DISCORD_TOKEN") { token } else {
		return Err(anyhow!("'DISCORD_TOKEN' was not found").into());
	};

	// Set gateway intents, which decides what events the bot will be notified about
	let intents = GatewayIntents::GUILDS 
		| GatewayIntents::GUILD_MESSAGES
		// | GatewayIntents::GUILD_MESSAGE_TYPING
		| GatewayIntents::MESSAGE_CONTENT;

	let client = Client::builder(&token, intents)
		.event_handler(Bot)
		.await
		.expect("Err creating client");



	Ok(client.into())
}
