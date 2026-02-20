create table if not exists verify_auth_replay (
  jti text primary key,
  sub text not null,
  scope text not null,
  exp bigint not null,
  consumed_at bigint not null
);

create index if not exists idx_verify_auth_replay_exp
  on verify_auth_replay(exp);
