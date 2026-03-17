alter table archive_parts
  add column if not exists raw_sha256 bytea;
