//! Model → price table (single source of truth per AGENTS §3 DRY).
//!
//! OpenCode stores `cost` directly in its SQLite DB, so this table is only
//! needed for Claude Code and Codex, which store raw token counts in their
//! JSONL transcripts. Dollar cost is derived: `tokens × price`.
//!
//! Prices are USD per **million** tokens, matching the providers' public
//! pricing pages as of 2026-07. Update when providers change pricing.

/// Per-million-token price for one model variant.
pub struct ModelPrice {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_read_per_mtok: f64,
    pub cache_write_per_mtok: f64,
}

impl ModelPrice {
    const ZERO: ModelPrice = ModelPrice {
        input_per_mtok: 0.0,
        output_per_mtok: 0.0,
        cache_read_per_mtok: 0.0,
        cache_write_per_mtok: 0.0,
    };
}

/// Look up the price for a model slug. Falls back to `ZERO` for
/// unknown / free-tier models (cost = 0 is correct in that case).
pub fn price_for(model: &str) -> ModelPrice {
    let m = model.to_lowercase();

    // ── Claude (Anthropic) ──
    if m.starts_with("claude-opus-4") || m.starts_with("claude-3-opus") {
        return ModelPrice {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
            cache_read_per_mtok: 1.50,
            cache_write_per_mtok: 18.75,
        };
    }
    if m.starts_with("claude-sonnet-4") || m.starts_with("claude-3-5-sonnet") {
        return ModelPrice {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: 0.30,
            cache_write_per_mtok: 3.75,
        };
    }
    if m.starts_with("claude-haiku-3") || m.starts_with("claude-3-haiku") {
        return ModelPrice {
            input_per_mtok: 0.80,
            output_per_mtok: 4.0,
            cache_read_per_mtok: 0.08,
            cache_write_per_mtok: 1.0,
        };
    }

    // ── GPT (OpenAI) ──
    if m.starts_with("gpt-4o-mini") {
        return ModelPrice {
            input_per_mtok: 0.15,
            output_per_mtok: 0.60,
            cache_read_per_mtok: 0.075,
            cache_write_per_mtok: 0.15,
        };
    }
    if m.starts_with("gpt-4o") {
        return ModelPrice {
            input_per_mtok: 2.50,
            output_per_mtok: 10.0,
            cache_read_per_mtok: 1.25,
            cache_write_per_mtok: 0.0,
        };
    }
    if m.starts_with("gpt-4-turbo") {
        return ModelPrice {
            input_per_mtok: 10.0,
            output_per_mtok: 30.0,
            cache_read_per_mtok: 0.0,
            cache_write_per_mtok: 0.0,
        };
    }
    if m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") {
        return ModelPrice {
            input_per_mtok: 15.0,
            output_per_mtok: 60.0,
            cache_read_per_mtok: 7.50,
            cache_write_per_mtok: 0.0,
        };
    }

    // ── Gemini (Google) ──
    if m.starts_with("gemini-2") || m.starts_with("gemini-1.5-pro") {
        return ModelPrice {
            input_per_mtok: 1.25,
            output_per_mtok: 5.0,
            cache_read_per_mtok: 0.3125,
            cache_write_per_mtok: 0.0,
        };
    }
    if m.starts_with("gemini-1.5-flash") {
        return ModelPrice {
            input_per_mtok: 0.075,
            output_per_mtok: 0.30,
            cache_read_per_mtok: 0.01875,
            cache_write_per_mtok: 0.0,
        };
    }

    // ── GLM (ZhipuAI / z.ai) ──
    if m.starts_with("glm-4") || m.starts_with("glm-5") {
        return ModelPrice {
            input_per_mtok: 0.10,
            output_per_mtok: 0.10,
            cache_read_per_mtok: 0.0,
            cache_write_per_mtok: 0.0,
        };
    }

    // ── DeepSeek ──
    if m.starts_with("deepseek") {
        return ModelPrice {
            input_per_mtok: 0.27,
            output_per_mtok: 1.10,
            cache_read_per_mtok: 0.07,
            cache_write_per_mtok: 0.27,
        };
    }

    ModelPrice::ZERO
}

/// Compute the dollar cost of one usage event, given the model slug and the
/// four token classes. Returns 0.0 for unknown/free models.
pub fn cost_usd(
    model: &str,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
) -> f64 {
    let p = price_for(model);
    let to_usd = |tokens: u64, per_m: f64| -> f64 {
        (tokens as f64 / 1_000_000.0) * per_m
    };
    to_usd(input, p.input_per_mtok)
        + to_usd(output, p.output_per_mtok)
        + to_usd(cache_read, p.cache_read_per_mtok)
        + to_usd(cache_write, p.cache_write_per_mtok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_sonnet_pricing() {
        let p = price_for("claude-sonnet-4-5-20250929");
        assert_eq!(p.input_per_mtok, 3.0);
        assert_eq!(p.output_per_mtok, 15.0);
    }

    #[test]
    fn claude_opus_pricing() {
        let p = price_for("claude-opus-4-20250929");
        assert_eq!(p.input_per_mtok, 15.0);
    }

    #[test]
    fn unknown_model_is_zero() {
        let p = price_for("some-random-model-xyz");
        assert_eq!(p.input_per_mtok, 0.0);
        assert_eq!(cost_usd("some-random-model-xyz", 1000000, 500000, 0, 0), 0.0);
    }

    #[test]
    fn cost_calculation() {
        // 1M input + 1M output on Claude Sonnet = $3 + $15 = $18
        let c = cost_usd("claude-sonnet-4-5", 1_000_000, 1_000_000, 0, 0);
        assert!((c - 18.0).abs() < 0.001);
    }

    #[test]
    fn case_insensitive() {
        let p = price_for("GPT-4O-2024-08-06");
        assert_eq!(p.input_per_mtok, 2.50);
    }
}
