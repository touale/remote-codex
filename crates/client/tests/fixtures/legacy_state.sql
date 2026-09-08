INSERT INTO installation(id) VALUES('legacy-install');
UPDATE scopes SET revision=4 WHERE id='global';
INSERT INTO scopes(id,revision) VALUES('legacy-server',7);
INSERT INTO scopes(id,revision) VALUES('other-server',2),('third-server',3);
INSERT INTO connections(id,name,endpoint) VALUES(
    'legacy-server','dev','{"host":"server","user":"root","port":2222}'
);
INSERT INTO connections(id,name,endpoint) VALUES
    ('other-server','other','{"host":"other","user":"root","port":2222}'),
    ('third-server','third','{"host":"third","user":"root","port":2222}');
INSERT INTO settings(scope,key,spelling,representation,value) VALUES
    ('global','env.no_proxy','env.no_proxy','plain','localhost'),
    ('global','background','background','plain','true'),
    ('global','env.EMPTY','env.EMPTY','plain',''),
    ('global','env.https_proxy','env.https_proxy','plain','http://global.example:7890'),
    ('global','env.OPENAI_API_KEY','env.OPENAI_API_KEY','secret','shared-reference'),
    ('legacy-server','workspace','workspace','plain','/previous/workspace'),
    ('legacy-server','background','background','plain','false'),
    ('legacy-server','env.OPENAI_API_KEY','env.OPENAI_API_KEY','secret','vault-reference');
INSERT INTO credentials(id,state) VALUES('vault-reference','active');
INSERT INTO credentials(id,state) VALUES('shared-reference','active');
INSERT INTO contexts(id,connection_id,workspace) VALUES
    ('terminal-one','legacy-server','/previous/terminal'),
    ('terminal-two','legacy-server','/previous/workspace'),
    ('unbound',NULL,'/unbound');
PRAGMA application_id=1380140120;
