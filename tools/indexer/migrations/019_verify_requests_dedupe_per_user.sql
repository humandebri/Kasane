alter table if exists verify_requests
  drop constraint if exists verify_requests_input_hash_key;

drop index if exists verify_requests_input_hash_key;

create unique index if not exists uq_verify_requests_submitted_input_hash
  on verify_requests(submitted_by, input_hash);
