"use client";

// どこで: Opsページの容量監視 / 何を: estimated/high/hard を時系列ラインで表示 / なぜ: prune効果と閾値逼迫を同時に把握するため

import { ColorType, LineSeries, createChart } from "lightweight-charts";
import { useEffect, useMemo, useRef } from "react";

type CapacityTrendPoint = {
  sampledAtMs: string;
  estimatedKeptBytes: string;
  highWaterBytes: string;
  hardEmergencyBytes: string;
};

type Props = {
  points: CapacityTrendPoint[];
};

export function CapacityTrendChart({ points }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const chartData = useMemo(() => {
    const byTime = new Map<number, { estimated: number; high: number; hard: number }>();
    for (const point of points) {
      const sampledAtMs = Number(point.sampledAtMs);
      const estimatedBytes = Number(point.estimatedKeptBytes);
      const highBytes = Number(point.highWaterBytes);
      const hardBytes = Number(point.hardEmergencyBytes);
      if (!Number.isFinite(sampledAtMs) || !Number.isFinite(estimatedBytes) || !Number.isFinite(highBytes) || !Number.isFinite(hardBytes)) {
        continue;
      }
      const time = Math.floor(sampledAtMs / 1000);
      byTime.set(time, { estimated: estimatedBytes, high: highBytes, hard: hardBytes });
    }
    const estimated: Array<{ time: number; value: number }> = [];
    const high: Array<{ time: number; value: number }> = [];
    const hard: Array<{ time: number; value: number }> = [];
    const times = Array.from(byTime.keys()).sort((a, b) => a - b);
    for (const time of times) {
      const row = byTime.get(time);
      if (!row) {
        continue;
      }
      estimated.push({ time, value: row.estimated });
      high.push({ time, value: row.high });
      hard.push({ time, value: row.hard });
    }
    return { estimated, high, hard };
  }, [points]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const chart = createChart(container, {
      width: container.clientWidth,
      height: 220,
      localization: {
        timeFormatter: (value: number) => new Date(value * 1000).toLocaleString(),
        priceFormatter: (value: number) => formatMegaBytes(value),
      },
      layout: {
        background: { color: "#ffffff", type: ColorType.Solid },
        textColor: "#334155",
      },
      grid: {
        vertLines: { color: "#e2e8f0" },
        horzLines: { color: "#e2e8f0" },
      },
      rightPriceScale: {
        borderColor: "#cbd5e1",
      },
      timeScale: {
        borderColor: "#cbd5e1",
        timeVisible: true,
        secondsVisible: false,
      },
    });

    const estimatedSeries = chart.addSeries(LineSeries, {
      color: "#0284c7",
      lineWidth: 2,
      title: "estimated",
    });
    const highSeries = chart.addSeries(LineSeries, {
      color: "#d97706",
      lineWidth: 2,
      title: "high",
    });
    const hardSeries = chart.addSeries(LineSeries, {
      color: "#e11d48",
      lineWidth: 2,
      title: "hard",
    });
    // @ts-expect-error Runtime accepts unix-seconds numeric timestamps for time values.
    estimatedSeries.setData(chartData.estimated);
    // @ts-expect-error Runtime accepts unix-seconds numeric timestamps for time values.
    highSeries.setData(chartData.high);
    // @ts-expect-error Runtime accepts unix-seconds numeric timestamps for time values.
    hardSeries.setData(chartData.hard);
    chart.timeScale().fitContent();

    const resizeObserver = new ResizeObserver((entries) => {
      const first = entries[0];
      if (!first) {
        return;
      }
      chart.applyOptions({ width: first.contentRect.width });
    });
    resizeObserver.observe(container);

    return () => {
      resizeObserver.disconnect();
      chart.remove();
    };
  }, [chartData]);

  return <div ref={containerRef} className="h-[220px] w-full rounded border border-slate-200 bg-white" />;
}

function formatMegaBytes(value: number): string {
  if (!Number.isFinite(value) || value < 0) {
    return "N/A";
  }
  const mb = value / (1024 * 1024);
  return `${mb.toFixed(2)} MB`;
}
