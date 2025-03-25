# Connectors

## OpenAI Connector

### Manual Instructions

To run the OpenAI Connector manually (e.g. for testing purposes), you can run the following as a script or in the interactive prompt from the root directory:

```python
import common
import connectors

connector = common.Connector(
    connector="openai",
    model_name="MODEL_NAME_HERE",
    model_tag="undefined",
    arguments=[
        {
            "api_key":"YOUR_API_KEY_HERE"
        }
    ]
)

client = connectors.openai.OpenAIConnector(connector=connector)

client.completion(
    content="def mult(x, y):\n\treturn x * y",
    language="Python",
    language_version="3.13.2"
)
```

Replace `YOUR_API_KEY_HERE` with an API key generated under the SCG organisation at [https://platform.openai.com/api-keys](https://platform.openai.com/api-keys) and `MODEL_NAME_HERE` with an appropriate model or the name of a fine-tuned model.

### Fine-Tuned Models

| Model Name | Notes |
| ---------- | ----- |
| `ft:gpt-4o-mini-2024-07-18:scg:docstrings:BEnEMYz9` | Based on `gpt-4o-mini-docstrings-proto.jsonl` |
