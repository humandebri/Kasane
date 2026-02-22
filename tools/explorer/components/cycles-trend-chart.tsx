"use client";

// どこで: OpsページのCycles Trend / 何を: lightweight-chartsで見やすい時系列ラインを描画 / なぜ: 既存SVGより操作性と可読性を上げるため

import { ColorType, LineSeries, createChart } from "lightweight-charts";
import { useEffect, useMemo, useRef } from "react";

type CyclesTrendPoint = {
  sampledAtMs: string;
  cycles: string;
};

type Props = {
  points: CyclesTrendPoint[];
};

const CYCLES_TRILLION_DIVISOR = 1_000_000_000_000;

export function CyclesTrendChart({ points }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const chartData = useMemo(() => {
    const byTime = new Map<number, number>();
    for (const point of points) {
      const sampledAtMs = Number(point.sampledAtMs);
      const value = Number(point.cycles);
      if (!Number.isFinite(sampledAtMs) || !Number.isFinite(value)) {
        continue;
      }
      // lightweight-charts は秒単位のUNIX timestampを期待する。
      const unixSec = Math.floor(sampledAtMs / 1000);
      byTime.set(unixSec, value);
    }
    const out: Array<{ time: number; value: number }> = [];
    for (const [time, value] of byTime) {
      out.push({ time, value });
    }
    out.sort((a, b) => a.time - b.time);
    return out;
  }, [points]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    const chart = createChart(container, {
      width: container.clientWidth,
      height: 180,
      localization: {
        timeFormatter: (value: number) => formatLocalDateTimeSec(value),
        priceFormatter: (value: number) => formatCyclesTrillion(value),
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
        tickMarkFormatter: (time: unknown) => formatChartLocalTick(time),
      },
      handleScroll: {
        mouseWheel: true,
        pressedMouseMove: true,
        vertTouchDrag: false,
        horzTouchDrag: true,
      },
    });

    const line = chart.addSeries(LineSeries, {
      color: "#0284c7",
      lineWidth: 2,
      crosshairMarkerVisible: true,
      lastValueVisible: true,
      priceLineVisible: true,
    });
    // lightweight-charts の Time はブランド型だが、実行時は UNIX 秒(number)で問題なく描画される。
    // @ts-expect-error Runtime accepts unix-seconds numeric timestamps for time values.
    line.setData(chartData);
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

  return <div ref={containerRef} className="h-44 w-full rounded border border-slate-200 bg-white" />;
}

function formatLocalDateTimeSec(value: number): string {
  if (!Number.isFinite(value)) {
    return "";
  }
  return new Date(value * 1000).toLocaleString();
}

function formatChartLocalTick(time: unknown): string {
  const unixSec = parseChartUnixSec(time);
  if (unixSec === null) {
    return "";
  }
  return formatLocalTimeSec(unixSec);
}

function parseChartUnixSec(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value !== "object" || value === null) {
    return null;
  }
  if ("timestamp" in value && typeof value.timestamp === "number" && Number.isFinite(value.timestamp)) {
    return value.timestamp;
  }
  if (
    "year" in value &&
    "month" in value &&
    "day" in value &&
    typeof value.year === "number" &&
    typeof value.month === "number" &&
    typeof value.day === "number"
  ) {
    return Math.floor(Date.UTC(value.year, value.month - 1, value.day) / 1000);
  }
  return null;
}

function formatLocalTimeSec(value: number): string {
  if (!Number.isFinite(value)) {
    return "";
  }
  return new Date(value * 1000).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function formatCyclesTrillion(value: number): string {
  if (!Number.isFinite(value)) {
    return "0.0000 T";
  }
  return `${(value / CYCLES_TRILLION_DIVISOR).toFixed(4)} T`;
}
