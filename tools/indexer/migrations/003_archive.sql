create table if not exists archive_parts (
  block_number integer primary key,
  path text not null,
  sha256 blob not null,
  size_bytes integer not null,
  raw_bytes integer not null,
  created_at integer not null
);
