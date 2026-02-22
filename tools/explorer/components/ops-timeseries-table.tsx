"use client";

import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "./ui/table";

type OpsTimeseriesPoint = {
  sampledAtMs: string;
  queueLen: string;
  cycles: string;
  totalSubmitted: string;
  totalIncluded: string;
  totalDropped: string;
  failureRate: number;
};

type Props = {
  points: OpsTimeseriesPoint[];
};

export function OpsTimeseriesTable({ points }: Props) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Time</TableHead>
          <TableHead>Queue</TableHead>
          <TableHead>Cycles</TableHead>
          <TableHead>Submitted</TableHead>
          <TableHead>Included</TableHead>
          <TableHead>Dropped</TableHead>
          <TableHead>Failure Rate</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {points.map((point) => (
          <TableRow key={point.sampledAtMs}>
            <TableCell>{formatLocalDateTime(point.sampledAtMs)}</TableCell>
            <TableCell>{point.queueLen}</TableCell>
            <TableCell>{point.cycles}</TableCell>
            <TableCell>{point.totalSubmitted}</TableCell>
            <TableCell>{point.totalIncluded}</TableCell>
            <TableCell>{point.totalDropped}</TableCell>
            <TableCell>{(point.failureRate * 100).toFixed(2)}%</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function formatLocalDateTime(sampledAtMs: string): string {
  const value = Number(sampledAtMs);
  if (!Number.isFinite(value)) {
    return "N/A";
  }
  const date = new Date(value);
  const year = date.getUTCFullYear();
  const month = (date.getUTCMonth() + 1).toString().padStart(2, "0");
  const day = date.getUTCDate().toString().padStart(2, "0");
  const hours = date.getUTCHours().toString().padStart(2, "0");
  const minutes = date.getUTCMinutes().toString().padStart(2, "0");
  const seconds = date.getUTCSeconds().toString().padStart(2, "0");
  return `${year}-${month}-${day} ${hours}:${minutes}:${seconds} UTC`;
}
