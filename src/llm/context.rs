pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        Message {
            role: role.to_string(),
            content: content.into(),
        }
    }
}

pub struct ChatConfig {
    pub model: String,
    pub api_key_env: Option<String>,
    pub api_url: Option<String>,
    pub messages: Vec<Message>,
}

impl ChatConfig {
    pub fn new() -> Self {
        ChatConfig {
            model: std::env::var("OPENAI_DEFAULT_MODEL")
                .unwrap_or_else(|_| "gpt-5.5".to_string()),
            api_key_env: None,
            api_url: None,
            messages: vec![],
        }
    }

    // Format:
    //   config: key\nvalue\x00 records, terminated by lone \x00
    //   messages: role\ncontent\x00 records
    pub fn parse(data: &[u8]) -> Self {
        let mut config = ChatConfig::new();

        let (cfg, msgs) = data
            .windows(2)
            .position(|w| w == [0, 0])
            .map(|p| (&data[..p], &data[p + 2..]))
            .unwrap_or((data, &[]));

        for rec in cfg.split(|&b| b == 0) {
            if let Some((k, v)) = String::from_utf8_lossy(rec).split_once('\n')
            {
                match k {
                    "model" => config.model = v.to_string(),
                    "api_key_env" => config.api_key_env = Some(v.to_string()),
                    "api_url" => config.api_url = Some(v.to_string()),
                    _ => {}
                }
            }
        }

        config.messages = msgs
            .split(|&b| b == 0)
            .filter(|r| !r.is_empty())
            .filter_map(|r| {
                String::from_utf8_lossy(r)
                    .split_once('\n')
                    .map(|(role, content)| Message::new(role, content))
            })
            .collect();

        config
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::new();
        fn rec(out: &mut Vec<u8>, k: &str, v: &str) {
            out.extend_from_slice(k.as_bytes());
            out.push(b'\n');
            out.extend_from_slice(v.as_bytes());
            out.push(0);
        }
        rec(&mut out, "model", &self.model);
        if let Some(v) = &self.api_key_env {
            rec(&mut out, "api_key_env", v);
        }
        if let Some(v) = &self.api_url {
            rec(&mut out, "api_url", v);
        }
        out.push(0);
        for m in &self.messages {
            rec(&mut out, &m.role, &m.content);
        }
        out
    }

    pub fn resolve_base_url(&self) -> String {
        let raw = self.api_url.clone().unwrap_or_else(|| {
            std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "openai".to_string())
        });
        match raw.as_str() {
            "openai" => "https://api.openai.com/v1".to_string(),
            "google" => {
                "https://generativelanguage.googleapis.com/v1beta/openai"
                    .to_string()
            }
            "openrouter" => "https://openrouter.ai/api/v1".to_string(),
            other => other.trim_end_matches('/').to_string(),
        }
    }

    pub fn resolve_api_key(&self) -> String {
        let env = self.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY");
        std::env::var(env).unwrap_or_else(|_| {
            eprintln!("env '{env}' is not set");
            std::process::exit(1);
        })
    }
}
