INSERT INTO installation(id) VALUES('execution-install');
INSERT INTO server_revisions(id,revision) VALUES('dev-id',11),('test-id',3);
INSERT INTO connections(id,name,endpoint,phase) VALUES
    ('dev-id','dev','{"host":"dev.example","user":"root","port":2222}','service_prepared'),
    ('test-id','test','{"host":"test.example","user":null,"port":null}','saved');
INSERT INTO settings(server,key,spelling,representation,value) VALUES
    ('dev-id','codex.update_policy','codex.update_policy','plain','manual'),
    ('dev-id','background','background','plain','false'),
    ('dev-id','disconnect_grace_seconds','disconnect_grace_seconds','plain','42'),
    ('dev-id','execution.mode','execution.mode','plain','unrestricted'),
    ('dev-id','proxy.mode','proxy.mode','plain','direct'),
    ('dev-id','env.SERVICE_TOKEN','env.SERVICE_TOKEN','secret','service-reference');
INSERT INTO credentials(id,state) VALUES('service-reference','active'),('legacy-reference','active');
INSERT INTO retained_legacy_credentials(id) VALUES('legacy-reference');
INSERT INTO server_access(server,identity_file,managed_public_key,ssh_config,remote_identity,service_executable,service_root,applied_revision,checked_at,health) VALUES
    ('dev-id','/keys/dev','ssh-ed25519 fixture','/ssh/config','remote-install','/runtimes/service/fixture/server','/execution-v3',11,123,'ready');
INSERT INTO workspace_history(server,path,used_at) VALUES('dev-id','/workspace/project',456);
INSERT INTO project_trust(server,path,digest) VALUES('dev-id','/workspace/project','trusted-digest');
INSERT INTO local_sessions(id,server,record,updated_at) VALUES('thread-id','dev-id',
    '{"server_id":"dev-id","remote_identity":"remote-install","environment_id":"rc_dev","codex_home":"/local/.codex","codex_version":"0.153.4","execution_mode":"unrestricted","revision":11,"session":{"id":"thread-id","title":"Existing work","cwd":"/workspace/project","created_at":1,"updated_at":2,"archived":false,"state":"ready"}}',789);
PRAGMA application_id=1380140120;
PRAGMA user_version=6;
