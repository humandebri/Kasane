create table if not exists verify_requests (
  id text primary key,
  contract_address text not null,
  chain_id integer not null,
  submitted_by text not null,
  status text not null,
  input_hash text not null unique,
  payload_compressed bytea not null,
  error_code text,
  error_message text,
  started_at bigint,
  finished_at bigint,
  attempts integer not null default 0,
  verified_contract_id text,
  created_at bigint not null,
  updated_at bigint not null
);

create index if not exists idx_verify_requests_status_created_at on verify_requests(status, created_at);
create index if not exists idx_verify_requests_contract_chain on verify_requests(contract_address, chain_id);
create index if not exists idx_verify_requests_submitted_created on verify_requests(submitted_by, created_at);

create table if not exists verify_blobs (
  id text primary key,
  sha256 text not null unique,
  encoding text not null,
  raw_size integer not null,
  blob bytea not null
);

create table if not exists verified_contracts (
  id text primary key,
  contract_address text not null,
  chain_id integer not null,
  contract_name text not null,
  compiler_version text not null,
  optimizer_enabled boolean not null,
  optimizer_runs integer not null,
  evm_version text,
  creation_match boolean not null,
  runtime_match boolean not null,
  abi_json text not null,
  source_blob_id text not null references verify_blobs(id),
  metadata_blob_id text not null references verify_blobs(id),
  published_at bigint not null,
  unique(contract_address, chain_id)
);

alter table if exists verify_requests
  add constraint verify_requests_verified_contract_fk
  foreign key (verified_contract_id)
  references verified_contracts(id);

create table if not exists verify_job_logs (
  id text primary key,
  request_id text not null references verify_requests(id) on delete cascade,
  level text not null,
  message text not null,
  created_at bigint not null
);

create index if not exists idx_verify_job_logs_request_created on verify_job_logs(request_id, created_at desc);
create index if not exists idx_verify_job_logs_created on verify_job_logs(created_at);
