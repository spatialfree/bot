use std::collections::HashMap;
use anyhow::anyhow;
use tracing::{error, info};

// use shuttle_service::Environment;
use shuttle_secrets::SecretStore;

use serenity::async_trait;
// use serenity::client;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::id::ChannelId;
use serenity::model::id::UserId;
use serenity::prelude::*;

use async_openai::{
	types::{ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs, Role},
};

// this is a temporary system for wiring things up
use lazy_static::lazy_static;
lazy_static! {
	static ref CHANNEL_PROMPTS: HashMap<ChannelId, ChannelId> = { 
		let mut map = HashMap::new();
		//         ChannelId(chat                  ChannelId(prompt
		map.insert(ChannelId(1103101252830765096), ChannelId(1110235580220063804));
		map.insert(ChannelId(1110858323647017010), ChannelId(1110858298137251852));
		map.insert(ChannelId(1110858562214834286), ChannelId(1110858542895878204));
		
		map.insert(ChannelId(1110861110921416766), ChannelId(1110861092474863678));
		map.insert(ChannelId(1110861205133864990), ChannelId(1110861187048030248));
		map.insert(ChannelId(1110861443047362663), ChannelId(1110861426546966618));

		map.insert(ChannelId(1113021245240389686), ChannelId(1113021188013297705));
		map.insert(ChannelId(1113021309111259216), ChannelId(1113021291843301456));
		map.insert(ChannelId(1113021365226848287), ChannelId(1113021346688024596));
		
		map.insert(ChannelId(1113021420893655091), ChannelId(1113021403566981140));
		map.insert(ChannelId(1113021465424576622), ChannelId(1113021448588644363));
		map.insert(ChannelId(1113021517844979762), ChannelId(1113021501264908369));

		map.insert(ChannelId(1113021558529732638), ChannelId(1113021542188724355));
		map.insert(ChannelId(1113021599826849892), ChannelId(1113021581728428114));
		map.insert(ChannelId(1113021637466525737), ChannelId(1113021620592844850));
		map.insert(ChannelId(1113021675768914001), ChannelId(1113021660317093889));

		// dev
		// map.insert(ChannelId(1103101223059587083), ChannelId(1110232499466018888));

		map
	};
}

struct Bot;

#[async_trait]
impl EventHandler for Bot {
	async fn message(&self, ctx: Context, msg: Message) {
		// stop it from looping back in on itself
		if msg.is_own(&ctx.cache) { 
			return;
		}
		
		let mut in_chat_thread = false;
		let mut system_prompt = "".to_string();
		let threads = msg.guild(&ctx.cache).unwrap().threads;
		for thread in threads {
			// if inside a matching thread
			if msg.channel_id == thread.id {
				// check hash map for prompt, if found then replace prompt
				if !in_chat_thread {
					if let Some(channel) = CHANNEL_PROMPTS.get(&thread.parent_id.unwrap()) {
						// overwrite the system_prompt using the message content in the prompt channel
						// async fn(self, impl AsRef<Http>, F) -> Result<Vec<Message, Global>, Error>
						system_prompt = channel.messages(&ctx.http, |retriever| {
							retriever.limit(1)
						}).await.unwrap().first().unwrap().content.clone();
						in_chat_thread = true;

						info!("system_prompt = {:?}", system_prompt);
					}
				}
			}
		}

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
			.content(system_prompt)
			.build()
			.unwrap());

		// info!("msgs: {:?}", msgs);
		

		// chat call
		let openai_client = async_openai::Client::new();
		let request = CreateChatCompletionRequestArgs::default()
			.max_tokens(1024u16) // bad default?
			.model("gpt-4")
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
		| GatewayIntents::MESSAGE_CONTENT;

	let client = Client::builder(&token, intents)
		.event_handler(Bot)
		.await
		.expect("Err creating client");

	Ok(client.into())
}