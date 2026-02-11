create table if not exists archive_parts (
  block_number bigint primary key,
  path text not null,
  sha256 bytea not null,
  size_bytes bigint not null,
  raw_bytes bigint not null,
  created_at bigint not null
);

create index if not exists idx_archive_parts_created_at_desc on archive_parts(created_at desc);
