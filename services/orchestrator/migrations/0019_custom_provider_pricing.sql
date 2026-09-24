-- What a user-added AI provider costs, in USD per 1,000 tokens, as the user read it from that provider's own price
-- list. Both are set or neither: a provider without them is treated as unmetered and its cost shows as $0.00,
-- exactly as before this migration. No price is ever guessed for an arbitrary endpoint, so a paid one only shows a
-- real cost once its price is entered.
ALTER TABLE custom_ai_providers ADD COLUMN input_usd_per_1k REAL;
ALTER TABLE custom_ai_providers ADD COLUMN output_usd_per_1k REAL;
