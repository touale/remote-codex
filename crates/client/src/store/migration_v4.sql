-- Freeze the former global settings into each server without replacing local values.
INSERT OR IGNORE INTO settings(scope,key,spelling,representation,value)
    SELECT c.id,s.key,s.spelling,s.representation,s.value
    FROM connections c CROSS JOIN settings s WHERE s.scope='global';

-- Historical built-in values are fixed here, independent of future product defaults.
-- In particular, existing servers must not silently change from inherit to direct.
INSERT OR IGNORE INTO settings(scope,key,spelling,representation,value)
    SELECT c.id,d.key,d.key,'plain',d.value FROM connections c CROSS JOIN (
        SELECT 'background' AS key,'true' AS value
        UNION ALL SELECT 'disconnect_grace_seconds','30'
        UNION ALL SELECT 'proxy.mode','inherit'
        UNION ALL SELECT 'codex.update_policy','manual'
    ) d;

DELETE FROM settings WHERE scope='global';
DELETE FROM scopes WHERE id='global';
ALTER TABLE scopes RENAME TO server_revisions;
ALTER TABLE settings RENAME COLUMN scope TO server;
