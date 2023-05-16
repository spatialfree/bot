use std::collections::HashMap;
use anyhow::anyhow;
use serenity::async_trait;
use serenity::client;
use serenity::model::channel::Message;
use serenity::model::channel::GuildChannel;
use serenity::model::gateway::Ready;
use serenity::model::id::ChannelId;
use serenity::model::id::UserId;
use serenity::prelude::*;
// use shuttle_service::Environment;
use shuttle_secrets::SecretStore;
use tracing::{error, info};


use async_openai::{
	types::{ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs, Role},
};

struct Bot;

struct PersistentData;

impl TypeMapKey for PersistentData {
	type Value = HashMap<String, String>;
}

static mut THREAD_IDS : String = String::new();


#[async_trait]
impl EventHandler for Bot {
	
	// async fn thread_list_sync()

	async fn thread_create(&self, ctx: Context, channel: GuildChannel) {
		// info!("thread: {:?}", channel);
		unsafe {
			let parent_id : ChannelId = std::env::var("PARENT_ID")
				.expect("Expected a parent id in the environment")
				.parse()
				.expect("The parent id was not a valid id");
			if channel.parent_id == Some(parent_id) && !THREAD_IDS.contains(&channel.id.to_string()) {
				THREAD_IDS += &channel.id.to_string();
				// info!("threads: {:?}", THREAD_IDS);
				if let Err(e) = channel.say(&ctx.http, "Hello!").await {
					error!("Error sending message: {:?}", e);
				}
			}
		}
	}

	async fn message(&self, ctx: Context, msg: Message) {
		unsafe {
			if !THREAD_IDS.contains(&msg.channel_id.to_string()) {
				return;
			}
		}

		if msg.author.bot { // a stand in to stop the bot from looping back in on itself
			return;
		}
		info!("msg received from: {:?}", msg.author.name);

		// show typing
		if let Err(e) = msg.channel_id.broadcast_typing(&ctx.http).await {
			error!("Error sending message: {:?}", e);
		}
		
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
			.content("You are a helpful assistant.")
			.build()
			.unwrap());

		// info!("msgs: {:?}", msgs);
		

		// chat call
		let openai_client = async_openai::Client::new();
		let request = CreateChatCompletionRequestArgs::default()
			.max_tokens(512u16) // bad default?
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


		if let Err(e) = msg.channel_id.say(&ctx.http, last_message).await {
			error!("Error sending message: {:?}", e);
		}
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

	// Get the parent channel id set in `Secrets.toml`
	let parent_id = if let Some(parent_id) = secret_store.get("PARENT_ID") { 
		parent_id } else {
		return Err(anyhow!("'PARENT_ID' was not found").into());
	};
	std::env::set_var("PARENT_ID", parent_id);

	// Get the openai apikey set in `Secrets.toml`
	let api_key = if let Some(api_key) = secret_store.get("OPENAI_API_KEY") { api_key } else {
		return Err(anyhow!("'OPENAI_API_KEY' was not found").into());
	};
	std::env::set_var("OPENAI_API_KEY", api_key);

	// Get the discord token set in `Secrets.toml`
	let token = if let Some(token) = secret_store.get("DISCORD_TOKEN") { token } else {
		return Err(anyhow!("'DISCORD_TOKEN' was not found").into());
	};

	// check if shuttle running locally *dev bot overwrites prod bot*
	// if Ok(secret_store.get("ENVIRONMENT")) {
	// 	bot_id = 
	// }

	// Set gateway intents, which decides what events the bot will be notified about
	let intents = GatewayIntents::GUILDS 
		| GatewayIntents::GUILD_MESSAGES
		// | GatewayIntents::GUILD_MESSAGE_TYPING
		| GatewayIntents::MESSAGE_CONTENT;

	let mut client = Client::builder(&token, intents).event_handler(Bot).await.map_err(|e| anyhow!(e))?; {
		let mut data = client.data.write().await;
		data.insert::<PersistentData>(HashMap::default());
		
		// insert bot_id into data
    // let persistent = data.get_mut::<PersistentData>().unwrap();
    // persistent.insert("bot_id".to_string(), bot_id.to_string());
	}
	// .expect("Err creating client");

	Ok(client.into())
}
