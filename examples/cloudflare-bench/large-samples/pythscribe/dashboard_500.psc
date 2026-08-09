R*
T*

@d
class Metric:
    name: str
    value: float
    delta: float
    unit: str
    category: str

@d
class Alert:
    id: int
    severity: str
    message: str
    timestamp: str
    acknowledged: bool

@d
class TimePoint:
    t: int
    value: float

def format_currency(value):
    if value >= 1000000:
        return f"${value / 1000000:.2f}M"
    if value >= 1000:
        return f"${value / 1000:.1f}K"
    return f"${value:.2f}"

def format_percent(value):
    sign = "+" if value >= 0 else ""
    return f"{sign}{value:.2f}%"

def severity_color(sev):
    if sev == "critical":
        return $c6
    if sev == "warning":
        return $c8
    if sev == "info":
        return $c4
    return $c3

def severity_bg(sev):
    if sev == "critical":
        return $cB
    if sev == "warning":
        return $cA
    if sev == "info":
        return $cD
    return $c5

@c
def MetricCard(metric):
    delta_color = $c9 if metric.delta >= 0 else $c6
    return div(className="metric-card", st={$pad: $p4, $bd: $br2, $brr: $p6})(div(className="metric-name", st={$fs: $p1, $col: $c3})(metric.name), div(className="metric-value", st={$fs: $p7, $fw: $w7})(f"{metric.value:.2f}", metric.unit), div(className="metric-delta", st={$fs: $p1, $col: delta_color})(format_percent(metric.delta)))


@c
def AlertRow(alert, on_acknowledge):
    bg = severity_bg(alert.severity)
    color = severity_color(alert.severity)

    def click():
        on_acknowledge(alert.id)

    return div(className="alert-row", st={$pad: $p1, $bg: bg, $blt: f"4px solid {color}", $mb: $p6, $dsp: "flex", $jc: $sbw, $ai: "center"})(div()(div(st={$fw: $w6, $col: color})(alert.severity.upper()), div(st={$fs: $p8})(alert.message), div(st={$fs: $p3, $col: $c1})(alert.timestamp)), button(oc=click, dis=alert.acknowledged, st={$pad: "6px 12px", $bd: $br1})("Acknowledge" if not alert.acknowledged else "Ack'd"))

@c
def Sparkline(points, width, height):
    if len(points) < 2:
        return svg(width=width, height=height)()

    max_v = points[0].value
    min_v = points[0].value
    for p in points:
        if p.value > max_v:
            max_v = p.value
        if p.value < min_v:
            min_v = p.value

    span = max_v - min_v
    if span == 0:
        span = 1

    coords = []
    for i, p in enumerate(points):
        x = (i / (len(points) - 1)) * width
        y = height - ((p.value - min_v) / span) * height
        coords.append(f"{x:.1f},{y:.1f}")

    polyline_points = " ".join(coords)
    return svg(width=width, height=height)(polyline(points=polyline_points, stroke=$c4, stroke_width=2, fill="none"))

@c
def CategoryFilter(active, on_change):
    categories = ["all", "performance", "errors", "traffic", "infrastructure"]

    def make_handler(cat):
        return lambda: on_change(cat)

    return div(className="category-filter", st={$dsp: "flex", "gap": $p6})(*[
        button(
            oc=make_handler(c),
            st={
                $pad: "8px 16px",
                $bg: $c4 if c == active else $cC,
                $col: "white" if c == active else $c7,
                $bd: "none",
                $brr: $p5,
                $cur: "pointer",
            },
        )(c.capitalize())
        for c in categories
    ])

@c
def SearchBar(value, on_change):
    def handle_change(e):
        on_change(e.target.value)

    return input(type="text", ph="Filter alerts...", value=value, oh=handle_change, st={$wd: $pct, $pad: "10px 14px", $bd: $br1, $brr: $p5, $fs: $p8})

@c
def StatGrid(metrics):
    return div(className="stat-grid", st={$dsp: "grid", $gtc: "repeat(auto-fill, minmax(220px, 1fr))", "gap": $p4})(*[MetricCard(metric=m) for m in metrics])

@c
def AlertList(alerts, on_acknowledge):
    if len(alerts) == 0:
        return div(st={$ta: "center", $pad: "32px", $col: $c1})("No alerts to display")
    return div()(*[AlertRow(alert=a, oa=on_acknowledge) for a in alerts])

@c
def TimelineChart(history, width, height):
    return div(className="timeline", st={$pad: $p4, $bd: $br2})(h3(st={$mar: "0 0 12px 0"})("Last hour"), Sparkline(points=history, width=width, height=height))

@c
def Header(title, refresh_at):
    return header(st={$dsp: "flex", $jc: $sbw, $pad: "16px 24px", $bb: $br2, $bg: $c2})(h1(st={$mar: 0, $fs: $pA})(title), div(st={$fs: $p1, $col: $c3})("Last refreshed: ", refresh_at))


@c
def Dashboard():
    metrics, set_metrics = us([])
    alerts, set_alerts = us([])
    history, set_history = us([])
    category, set_category = us("all")
    search, set_search = us("")
    refresh, set_refresh = us("--:--:--")

    def fetch_data():
        new_metrics = [
            Metric(name="Requests/s", value=1247.0, delta=3.4, unit="", category="traffic"),
            Metric(name="P99 Latency", value=124.0, delta=-2.1, unit="ms", category="performance"),
            Metric(name="Error Rate", value=0.32, delta=0.05, unit="%", category="errors"),
            Metric(name="CPU", value=68.0, delta=1.2, unit="%", category="infrastructure"),
            Metric(name="Memory", value=72.5, delta=0.8, unit="%", category="infrastructure"),
            Metric(name="Cache Hit", value=94.2, delta=-0.3, unit="%", category="performance"),
            Metric(name="DB Queries/s", value=823.0, delta=5.1, unit="", category="traffic"),
            Metric(name="Open Sockets", value=4218.0, delta=2.7, unit="", category="infrastructure"),
        ]
        new_alerts = [
            Alert(id=1, severity="critical", message="DB connection pool exhausted on shard-3", timestamp="14:23:01", acknowledged=False),
            Alert(id=2, severity="warning", message="P99 latency exceeded threshold (210ms)", timestamp="14:21:44", acknowledged=False),
            Alert(id=3, severity="info", message="Auto-scaler added 2 instances", timestamp="14:18:12", acknowledged=True),
            Alert(id=4, severity="warning", message="Memory pressure on api-prod-7", timestamp="14:15:33", acknowledged=False),
        ]
        new_history = [TimePoint(t=i, value=100.0 + i * 0.5 + (i % 7) * 8.0) for i in range(60)]
        set_metrics(new_metrics)
        set_alerts(new_alerts)
        set_history(new_history)
        set_refresh("14:24:18")

    ue(fetch_data, [])

    def acknowledge(alert_id):
        updated = []
        for a in alerts:
            if a.id == alert_id:
                updated.append(Alert(
                    id=a.id, severity=a.severity, message=a.message,
                    timestamp=a.timestamp, acknowledged=True,
                ))
            else:
                updated.append(a)
        set_alerts(updated)

    def filter_metrics():
        if category == "all":
            return metrics
        result = []
        for m in metrics:
            if m.category == category:
                result.append(m)
        return result

    def filter_alerts():
        if search == "":
            return alerts
        result = []
        s = search.lower()
        for a in alerts:
            if s in a.message.lower() or s in a.severity:
                result.append(a)
        return result

    visible_metrics = filter_metrics()
    visible_alerts = filter_alerts()

    return div(
        cl="dashboard",
        st={"font_family": "system-ui, sans-serif", $bg: $c5, "min_height": $vh},
    )(
        Header(title="Operations Dashboard", refresh_at=refresh),
        main(st={$pad: $p7, "max_width": "1280px", $mar: "0 auto"})(
            section(st={$mb: $p7})(
                CategoryFilter(active=category, oh=set_category),
            ),
            section(st={$mb: $p7})(
                StatGrid(metrics=visible_metrics),
            ),
            section(st={$mb: $p7})(
                TimelineChart(history=history, width=720, height=140),
            ),
            section()(
                div(st={$mb: $p1})(
                    SearchBar(value=search, oh=set_search),
                ),
                AlertList(alerts=visible_alerts, oa=acknowledge),
            ),
        ),
    )
