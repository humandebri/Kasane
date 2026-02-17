"use client";

// どこで: OpsページのCycles Trend / 何を: lightweight-chartsで見やすい時系列ラインを描画 / なぜ: 既存SVGより操作性と可読性を上げるため

import { ColorType, LineSeries, createOptionsChart } from "lightweight-charts";
import { useEffect, useMemo, useRef } from "react";

type CyclesTrendPoint = {
  sampledAtMs: string;
  cycles: string;
};

type Props = {
  points: CyclesTrendPoint[];
};

export function CyclesTrendChart({ points }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const chartData = useMemo(() => {
    const out: Array<{ time: number; value: number }> = [];
    for (const point of points) {
      const sampledAtMs = Number(point.sampledAtMs);
      const value = Number(point.cycles);
      if (!Number.isFinite(sampledAtMs) || !Number.isFinite(value)) {
        continue;
      }
      // lightweight-charts は秒単位のUNIX timestampを期待する。
      out.push({ time: Math.floor(sampledAtMs / 1000), value });
    }
    out.sort((a, b) => a.time - b.time);
    return out;
  }, [points]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    const chart = createOptionsChart(container, {
      width: container.clientWidth,
      height: 180,
      localization: {
        timeFormatter: (value: number) => formatLocalDateTimeSec(value),
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
        timeVisible: false,
        secondsVisible: false,
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
