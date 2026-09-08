-- Preserve legacy directory choices before removing the terminal-context model.
-- Existing recent-directory timestamps remain authoritative.
INSERT OR IGNORE INTO workspace_history(server,path)
    SELECT settings.scope,settings.value FROM settings
    JOIN connections ON connections.id=settings.scope
    WHERE key='workspace' AND representation='plain' AND value LIKE '/%';
INSERT OR IGNORE INTO workspace_history(server,path)
    SELECT connection_id,workspace FROM contexts
    JOIN connections ON connections.id=contexts.connection_id
    WHERE workspace LIKE '/%';
DELETE FROM settings WHERE key='workspace';
DROP TABLE contexts;
