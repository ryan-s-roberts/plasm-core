// Every `openai-generic` client targets OpenRouter (OpenAI-compatible API). Requires OPENROUTER_API_KEY.

client<llm> Sonnet {
    provider openai-generic
    options {
        base_url "https://openrouter.ai/api/v1"
        model "anthropic/claude-3.5-sonnet"
        api_key env.OPENROUTER_API_KEY
        temperature 0
        seed 42
    }
}

client<llm> GPT4o {
    provider openai-generic
    options {
        base_url "https://openrouter.ai/api/v1"
        model "openai/gpt-4o"
        api_key env.OPENROUTER_API_KEY
        temperature 0
        seed 42
    }
}

client<llm> Fast {
    provider openai-generic
    options {
        base_url "https://openrouter.ai/api/v1"
        model "anthropic/claude-3.5-haiku"
        api_key env.OPENROUTER_API_KEY
        temperature 0
        seed 42
    }
}

client<llm> DeepExtract {
    provider openai-generic
    options {
        base_url "https://openrouter.ai/api/v1"
        model "anthropic/claude-3.5-sonnet"
        api_key env.OPENROUTER_API_KEY
        max_tokens 4096
        temperature 0
        seed 42
    }
}
