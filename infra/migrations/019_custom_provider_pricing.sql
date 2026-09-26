-- Target-schema mirror of services/orchestrator/migrations/0019_custom_provider_pricing.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale).
--
-- What a user-added AI provider costs, in USD per 1,000 tokens, as the user
-- read it from that provider's own price list. Both are set or neither; NULL
-- means the provider is treated as unmetered.
ALTER TABLE custom_ai_providers ADD COLUMN input_usd_per_1k NUMERIC;
ALTER TABLE custom_ai_providers ADD COLUMN output_usd_per_1k NUMERIC;
