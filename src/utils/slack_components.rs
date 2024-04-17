use serde_json::json;
use serde_json::Value;

pub fn header_block(string: &str) -> Value {
    json!(
        {
            "type": "header",
            "text": {
                "type": "plain_text",
                "text": string,
                "emoji": true
            }
        }
    )
}

pub fn section_block(string: &str) -> Value {
    json!(
        {
			"type": "section",
			"text": {
				"type": "mrkdwn",
				"text": string
			}
		}
    )
}

pub fn context_block(string: &str) -> Value {
    json!(
        {
			"type": "context",
			"elements": [
				{
					"type": "mrkdwn",
					"text": string
				}
			]
		}
    )
}

pub fn divider_block() -> Value {
    json!(
        {
			"type": "divider"
		}
    )
}

pub fn list_block(args: Vec<Value>) -> Value {
    json!(
        {
            "type": "rich_text",
            "elements": [
                {
                    "type": "rich_text_list",
                    "elements": args
                        .iter()
                        .map(|arg| json!({
                            "type": "rich_text_section",
                            "elements": arg
                        }))
                        .collect::<Vec<Value>>(),
                    "style": "bullet",
                    "indent": 0,
                    "border": 0
                },
            ],
        } 
    )
}

pub fn text_element(text: &str, style: Option<Value>) -> serde_json::Value {
    json!({
        "type": "text",
        "text": text,
        "style": style.unwrap_or(json!({}))
    })
}

pub fn link_element(url: &str, text: &str, style: Option<Value>) -> serde_json::Value {
    json!({
        "type": "link",
        "url": url,
        "text": text,
        "style": style.unwrap_or(json!({}))
    })
}

pub fn user_element(user_id: &str) -> serde_json::Value {
    json!({
        "type": "user",
        "user_id": user_id
    })
}

pub fn button_block(label: &str, action_id: &str, value: &str) -> Value {
    json!({
        "type": "actions",
        "elements": [
            {
                "type": "button",
                "text": {
                    "type": "plain_text",
                    "text": label,
                    "emoji": true
                },
                "action_id": action_id,
                "value": value
            }
        ]
    })
}