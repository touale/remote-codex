UPDATE server_access SET identity_file='/keys/dev',remote_identity='remote-one',
    service_executable='/service/bin',service_root='/service',applied_revision=7
    WHERE server='legacy-server';
UPDATE workspace_history SET used_at=123 WHERE path='/previous/workspace';
INSERT INTO server_histories(server,remote_identity,history_home)
    VALUES('legacy-server','remote-one','/root/.codex');
INSERT INTO session_cache(remote_identity,history_home,id,record)
    VALUES('remote-one','/root/.codex','native-thread',
    '{"id":"native-thread","session_id":null,"history_home":"/root/.codex","title":"Native history","cwd":"/project","created_at":1,"updated_at":2,"archived":false,"imported":true,"source":"cli","state":"external","resumable":false,"unavailable_reason":"exclusive ownership unavailable"}');
