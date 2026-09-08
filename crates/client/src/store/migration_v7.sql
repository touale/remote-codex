-- The policy had no execution consumer. Do not retain it as a hidden setting.
DELETE FROM settings WHERE key='codex.update_policy';
-- Every previous effective configuration included the default policy, even
-- without a saved override. A new revision lets the remote mirror replace it.
-- The existing nonnegative CHECK rejects overflow and rolls back migration.
UPDATE server_revisions SET revision=CASE
    WHEN revision<9223372036854775807 THEN revision+1 ELSE -1 END;
