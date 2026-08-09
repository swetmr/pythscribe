import React, { useCallback, useEffect, useMemo, useState } from "react";

interface Metric {
  name: string;
  value: number;
  delta: number;
  unit: string;
  category: string;
}

interface Alert {
  id: number;
  severity: string;
  message: string;
  timestamp: string;
  acknowledged: boolean;
}

interface TimePoint {
  t: number;
  value: number;
}

function formatCurrency(value: number): string {
  if (value >= 1_000_000) {
    return `$${(value / 1_000_000).toFixed(2)}M`;
  }
  if (value >= 1_000) {
    return `$${(value / 1_000).toFixed(1)}K`;
  }
  return `$${value.toFixed(2)}`;
}

function formatPercent(value: number): string {
  const sign = value >= 0 ? "+" : "";
  return `${sign}${value.toFixed(2)}%`;
}

function severityColor(sev: string): string {
  if (sev === "critical") return "#dc2626";
  if (sev === "warning") return "#f59e0b";
  if (sev === "info") return "#3b82f6";
  return "#6b7280";
}

function severityBg(sev: string): string {
  if (sev === "critical") return "#fef2f2";
  if (sev === "warning") return "#fffbeb";
  if (sev === "info") return "#eff6ff";
  return "#f9fafb";
}

interface MetricCardProps {
  metric: Metric;
}

function MetricCard({ metric }: MetricCardProps): JSX.Element {
  const deltaColor = metric.delta >= 0 ? "#16a34a" : "#dc2626";
  return (
    <div
      className="metric-card"
      style={{
        padding: "16px",
        border: "1px solid #e5e7eb",
        borderRadius: "8px",
      }}
    >
      <div
        className="metric-name"
        style={{ fontSize: "12px", color: "#6b7280" }}
      >
        {metric.name}
      </div>
      <div
        className="metric-value"
        style={{ fontSize: "24px", fontWeight: "700" }}
      >
        {metric.value.toFixed(2)}
        {metric.unit}
      </div>
      <div
        className="metric-delta"
        style={{ fontSize: "12px", color: deltaColor }}
      >
        {formatPercent(metric.delta)}
      </div>
    </div>
  );
}

interface AlertRowProps {
  alert: Alert;
  onAcknowledge: (id: number) => void;
}

function AlertRow({ alert, onAcknowledge }: AlertRowProps): JSX.Element {
  const bg = severityBg(alert.severity);
  const color = severityColor(alert.severity);

  const handleClick = useCallback(() => {
    onAcknowledge(alert.id);
  }, [alert.id, onAcknowledge]);

  return (
    <div
      className="alert-row"
      style={{
        padding: "12px",
        background: bg,
        borderLeft: `4px solid ${color}`,
        marginBottom: "8px",
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
      }}
    >
      <div>
        <div style={{ fontWeight: "600", color: color }}>
          {alert.severity.toUpperCase()}
        </div>
        <div style={{ fontSize: "14px" }}>{alert.message}</div>
        <div style={{ fontSize: "11px", color: "#9ca3af" }}>
          {alert.timestamp}
        </div>
      </div>
      <button
        onClick={handleClick}
        disabled={alert.acknowledged}
        style={{ padding: "6px 12px", border: "1px solid #d1d5db" }}
      >
        {alert.acknowledged ? "Ack'd" : "Acknowledge"}
      </button>
    </div>
  );
}

interface SparklineProps {
  points: TimePoint[];
  width: number;
  height: number;
}

function Sparkline({ points, width, height }: SparklineProps): JSX.Element {
  if (points.length < 2) {
    return <svg width={width} height={height}></svg>;
  }

  let maxV = points[0].value;
  let minV = points[0].value;
  for (const p of points) {
    if (p.value > maxV) maxV = p.value;
    if (p.value < minV) minV = p.value;
  }

  let span = maxV - minV;
  if (span === 0) span = 1;

  const coords: string[] = [];
  for (let i = 0; i < points.length; i++) {
    const x = (i / (points.length - 1)) * width;
    const y = height - ((points[i].value - minV) / span) * height;
    coords.push(`${x.toFixed(1)},${y.toFixed(1)}`);
  }

  const polylinePoints = coords.join(" ");
  return (
    <svg width={width} height={height}>
      <polyline
        points={polylinePoints}
        stroke="#3b82f6"
        strokeWidth={2}
        fill="none"
      />
    </svg>
  );
}

interface CategoryFilterProps {
  active: string;
  onChange: (cat: string) => void;
}

function CategoryFilter({ active, onChange }: CategoryFilterProps): JSX.Element {
  const categories = ["all", "performance", "errors", "traffic", "infrastructure"];
  return (
    <div className="category-filter" style={{ display: "flex", gap: "8px" }}>
      {categories.map((c) => (
        <button
          key={c}
          onClick={() => onChange(c)}
          style={{
            padding: "8px 16px",
            background: c === active ? "#3b82f6" : "#f3f4f6",
            color: c === active ? "white" : "#374151",
            border: "none",
            borderRadius: "6px",
            cursor: "pointer",
          }}
        >
          {c.charAt(0).toUpperCase() + c.slice(1)}
        </button>
      ))}
    </div>
  );
}

interface SearchBarProps {
  value: string;
  onChange: (v: string) => void;
}

function SearchBar({ value, onChange }: SearchBarProps): JSX.Element {
  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    onChange(e.target.value);
  };

  return (
    <input
      type="text"
      placeholder="Filter alerts..."
      value={value}
      onChange={handleChange}
      style={{
        width: "100%",
        padding: "10px 14px",
        border: "1px solid #d1d5db",
        borderRadius: "6px",
        fontSize: "14px",
      }}
    />
  );
}

interface StatGridProps {
  metrics: Metric[];
}

function StatGrid({ metrics }: StatGridProps): JSX.Element {
  return (
    <div
      className="stat-grid"
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
        gap: "16px",
      }}
    >
      {metrics.map((m) => (
        <MetricCard key={m.name} metric={m} />
      ))}
    </div>
  );
}

interface AlertListProps {
  alerts: Alert[];
  onAcknowledge: (id: number) => void;
}

function AlertList({ alerts, onAcknowledge }: AlertListProps): JSX.Element {
  if (alerts.length === 0) {
    return (
      <div
        style={{ textAlign: "center", padding: "32px", color: "#9ca3af" }}
      >
        No alerts to display
      </div>
    );
  }
  return (
    <div>
      {alerts.map((a) => (
        <AlertRow key={a.id} alert={a} onAcknowledge={onAcknowledge} />
      ))}
    </div>
  );
}

interface TimelineChartProps {
  history: TimePoint[];
  width: number;
  height: number;
}

function TimelineChart({
  history,
  width,
  height,
}: TimelineChartProps): JSX.Element {
  return (
    <div
      className="timeline"
      style={{ padding: "16px", border: "1px solid #e5e7eb" }}
    >
      <h3 style={{ margin: "0 0 12px 0" }}>Last hour</h3>
      <Sparkline points={history} width={width} height={height} />
    </div>
  );
}

interface HeaderProps {
  title: string;
  refreshAt: string;
}

function Header({ title, refreshAt }: HeaderProps): JSX.Element {
  return (
    <header
      style={{
        display: "flex",
        justifyContent: "space-between",
        padding: "16px 24px",
        borderBottom: "1px solid #e5e7eb",
        background: "#ffffff",
      }}
    >
      <h1 style={{ margin: 0, fontSize: "20px" }}>{title}</h1>
      <div style={{ fontSize: "12px", color: "#6b7280" }}>
        Last refreshed: {refreshAt}
      </div>
    </header>
  );
}

export function Dashboard(): JSX.Element {
  const [metrics, setMetrics] = useState<Metric[]>([]);
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [history, setHistory] = useState<TimePoint[]>([]);
  const [category, setCategory] = useState("all");
  const [search, setSearch] = useState("");
  const [refresh, setRefresh] = useState("--:--:--");

  const fetchData = useCallback(() => {
    const newMetrics: Metric[] = [
      {
        name: "Requests/s",
        value: 1247.0,
        delta: 3.4,
        unit: "",
        category: "traffic",
      },
      {
        name: "P99 Latency",
        value: 124.0,
        delta: -2.1,
        unit: "ms",
        category: "performance",
      },
      {
        name: "Error Rate",
        value: 0.32,
        delta: 0.05,
        unit: "%",
        category: "errors",
      },
      {
        name: "CPU",
        value: 68.0,
        delta: 1.2,
        unit: "%",
        category: "infrastructure",
      },
      {
        name: "Memory",
        value: 72.5,
        delta: 0.8,
        unit: "%",
        category: "infrastructure",
      },
      {
        name: "Cache Hit",
        value: 94.2,
        delta: -0.3,
        unit: "%",
        category: "performance",
      },
      {
        name: "DB Queries/s",
        value: 823.0,
        delta: 5.1,
        unit: "",
        category: "traffic",
      },
      {
        name: "Open Sockets",
        value: 4218.0,
        delta: 2.7,
        unit: "",
        category: "infrastructure",
      },
    ];
    const newAlerts: Alert[] = [
      {
        id: 1,
        severity: "critical",
        message: "DB connection pool exhausted on shard-3",
        timestamp: "14:23:01",
        acknowledged: false,
      },
      {
        id: 2,
        severity: "warning",
        message: "P99 latency exceeded threshold (210ms)",
        timestamp: "14:21:44",
        acknowledged: false,
      },
      {
        id: 3,
        severity: "info",
        message: "Auto-scaler added 2 instances",
        timestamp: "14:18:12",
        acknowledged: true,
      },
      {
        id: 4,
        severity: "warning",
        message: "Memory pressure on api-prod-7",
        timestamp: "14:15:33",
        acknowledged: false,
      },
    ];
    const newHistory: TimePoint[] = [];
    for (let i = 0; i < 60; i++) {
      newHistory.push({ t: i, value: 100.0 + i * 0.5 + (i % 7) * 8.0 });
    }
    setMetrics(newMetrics);
    setAlerts(newAlerts);
    setHistory(newHistory);
    setRefresh("14:24:18");
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const acknowledge = useCallback(
    (alertId: number) => {
      const updated: Alert[] = [];
      for (const a of alerts) {
        if (a.id === alertId) {
          updated.push({
            id: a.id,
            severity: a.severity,
            message: a.message,
            timestamp: a.timestamp,
            acknowledged: true,
          });
        } else {
          updated.push(a);
        }
      }
      setAlerts(updated);
    },
    [alerts],
  );

  const visibleMetrics = useMemo(() => {
    if (category === "all") return metrics;
    const result: Metric[] = [];
    for (const m of metrics) {
      if (m.category === category) {
        result.push(m);
      }
    }
    return result;
  }, [metrics, category]);

  const visibleAlerts = useMemo(() => {
    if (search === "") return alerts;
    const result: Alert[] = [];
    const s = search.toLowerCase();
    for (const a of alerts) {
      if (a.message.toLowerCase().includes(s) || a.severity.includes(s)) {
        result.push(a);
      }
    }
    return result;
  }, [alerts, search]);

  return (
    <div
      className="dashboard"
      style={{
        fontFamily: "system-ui, sans-serif",
        background: "#f9fafb",
        minHeight: "100vh",
      }}
    >
      <Header title="Operations Dashboard" refreshAt={refresh} />
      <main style={{ padding: "24px", maxWidth: "1280px", margin: "0 auto" }}>
        <section style={{ marginBottom: "24px" }}>
          <CategoryFilter active={category} onChange={setCategory} />
        </section>
        <section style={{ marginBottom: "24px" }}>
          <StatGrid metrics={visibleMetrics} />
        </section>
        <section style={{ marginBottom: "24px" }}>
          <TimelineChart history={history} width={720} height={140} />
        </section>
        <section>
          <div style={{ marginBottom: "12px" }}>
            <SearchBar value={search} onChange={setSearch} />
          </div>
          <AlertList alerts={visibleAlerts} onAcknowledge={acknowledge} />
        </section>
      </main>
    </div>
  );
}
