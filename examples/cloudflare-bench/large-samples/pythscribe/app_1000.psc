R*
A*
T+

@d
class Customer:
    id: int
    name: str = Field(min_length=2, max_length=100)
    email: str = Field(email=True)
    phone: str = Field(min_length=5)
    status: str
    plan: str
    monthly_value: float = Field(ge=0.0)
    created_at: str
    notes: str

@d
class CustomerDraft:
    name: str = Field(min_length=2, max_length=100)
    email: str = Field(email=True)
    phone: str
    status: str
    plan: str
    monthly_value: float = Field(ge=0.0)
    notes: str

@d
class Activity:
    id: int
    customer_id: int
    kind: str
    summary: str
    timestamp: str

@d
class Page:
    items: list
    total: int
    page: int
    page_size: int

@d
class FilterState:
    query: str
    status: str
    plan: str
    sort_by: str
    sort_dir: str

def validate_email(email):
    if "@" not in email:
        return "Email must contain @"
    if "." not in email.split("@")[1]:
        return "Email domain looks invalid"
    return ""

def validate_required(value, label):
    if value == "" or value is None:
        return f"{label} is required"
    return ""

def validate_phone(phone):
    digits = ""
    for c in phone:
        if c.isdigit():
            digits = digits + c
    if len(digits) < 7:
        return "Phone must have at least 7 digits"
    return ""

def validate_money(value):
    if value < 0:
        return "Amount must be non-negative"
    if value > 1000000:
        return "Amount looks too large; double-check"
    return ""

def validate_draft(draft):
    errors = {}
    e = validate_required(draft.name, "Name")
    if e != "":
        errors["name"] = e
    e = validate_email(draft.email)
    if e != "":
        errors["email"] = e
    e = validate_phone(draft.phone)
    if e != "":
        errors["phone"] = e
    e = validate_money(draft.monthly_value)
    if e != "":
        errors["monthly_value"] = e
    return errors

async def api_list_customers(filters, page, page_size):
    await sleep(0.05)
    base = []
    for i in range(120):
        base.append(Customer(
            id=i + 1,
            name=f"Customer {i + 1}",
            email=f"c{i + 1}@example.com",
            phone=f"+1-555-{1000 + i:04d}",
            status="active" if i % 3 != 0 else "trial" if i % 3 == 1 else "churned",
            plan="enterprise" if i % 5 == 0 else "pro" if i % 2 == 0 else "starter",
            monthly_value=49.0 + (i % 11) * 50.0,
            created_at=f"2026-{(i % 12) + 1:02d}-{(i % 28) + 1:02d}",
            notes="",
        ))
    filtered = []
    q = filters.query.lower()
    for c in base:
        if filters.status != "all" and c.status != filters.status:
            continue
        if filters.plan != "all" and c.plan != filters.plan:
            continue
        if q != "":
            if q not in c.name.lower() and q not in c.email.lower():
                continue
        filtered.append(c)

    if filters.sort_by == "name":
        filtered = sorted(filtered, key=lambda x: x.name)
    elif filters.sort_by == "monthly_value":
        filtered = sorted(filtered, key=lambda x: x.monthly_value)
    elif filters.sort_by == "created_at":
        filtered = sorted(filtered, key=lambda x: x.created_at)

    if filters.sort_dir == "desc":
        filtered = list(reversed(filtered))

    start = page * page_size
    end = start + page_size
    return Page(items=filtered[start:end], total=len(filtered), page=page, page_size=page_size)

async def api_get_customer(customer_id):
    await sleep(0.03)
    return Customer(
        id=customer_id,
        name=f"Customer {customer_id}",
        email=f"c{customer_id}@example.com",
        phone=f"+1-555-{1000 + customer_id - 1:04d}",
        status="active",
        plan="pro",
        monthly_value=149.0,
        created_at="2026-01-15",
        notes="Long-time customer; renewal upcoming.",
    )

async def api_save_customer(draft, existing_id):
    await sleep(0.08)
    new_id = existing_id if existing_id is not None else 99999
    return Customer(
        id=new_id,
        name=draft.name,
        email=draft.email,
        phone=draft.phone,
        status=draft.status,
        plan=draft.plan,
        monthly_value=draft.monthly_value,
        created_at="2026-05-07",
        notes=draft.notes,
    )

async def api_delete_customer(customer_id):
    await sleep(0.04)
    return True

async def api_get_activity(customer_id):
    await sleep(0.04)
    return [
        Activity(id=1, customer_id=customer_id, kind="login", summary="Logged in", timestamp="14:23"),
        Activity(id=2, customer_id=customer_id, kind="upgrade", summary="Upgraded to pro", timestamp="13:10"),
        Activity(id=3, customer_id=customer_id, kind="payment", summary="Paid invoice #5021", timestamp="2026-05-01"),
    ]

def parse_route(hash_str):
    if hash_str == "" or hash_str == "#":
        return ("list", None)
    h = hash_str.lstrip("#").lstrip("/")
    parts = h.split("/")
    if parts[0] == "customers":
        if len(parts) == 1:
            return ("list", None)
        if len(parts) == 2 and parts[1] == "new":
            return ("create", None)
        if len(parts) == 2:
            cid = parts[1]
            if cid.isdigit():
                return ("detail", int(cid))
        if len(parts) == 3 and parts[2] == "edit":
            cid = parts[1]
            if cid.isdigit():
                return ("edit", int(cid))
    return ("list", None)

def route_to_hash(name, customer_id):
    if name == "list":
        return "#/customers"
    if name == "create":
        return "#/customers/new"
    if name == "detail":
        return f"#/customers/{customer_id}"
    if name == "edit":
        return f"#/customers/{customer_id}/edit"
    return "#/customers"

@c
def Field(text, error, children):
    error_color = $c6
    return div(st={$mb: $p1})(
        label(st={$dsp: "block", $fs: $p1, $col: $c7, $mb: "4px"})(text),
        children,
        div(st={$fs: $p3, $col: error_color, $mt: "4px", $mh: "14px"})(
            error if error != "" else "",
        ),
    )

@c
def TextInput(value, on_change, placeholder):
    def handle(e):
        on_change(e.target.value)
    return input(type="text", value=value, ph=placeholder, oh=handle, st={$wd: $pct, $pad: "8px 10px", $bd: $br1, $brr: $p5})

@c
def NumberInput(value, on_change, placeholder):
    def handle(e):
        try:
            on_change(float(e.target.value))
        except ValueError:
            on_change(0.0)
    return input(type="number", value=value, ph=placeholder, oh=handle, st={$wd: $pct, $pad: "8px 10px", $bd: $br1, $brr: $p5})

@c
def Select(value, options, on_change):
    def handle(e):
        on_change(e.target.value)
    return select(value=value, oh=handle, st={$wd: $pct, $pad: "8px 10px", $bd: $br1, $brr: $p5})(*[option(value=o, key=o)(o) for o in options])

@c
def Button(label, on_click, kind):
    bg = $c4 if kind == "primary" else $c2 if kind == "default" else $c6
    color = $c2 if kind != "default" else $c7
    return button(oc=on_click, st={$pad: "8px 16px", $bg: bg, $col: color, $bd: $br1, $brr: $p5, $cur: "pointer"})(label)

@c
def Toolbar(children):
    return div(st={$dsp: "flex", "gap": $p6, $pad: "12px 0"})(*children)

@c
def Card(children):
    return div(st={$pad: $p4, $bd: $br2, $brr: $p6, $bg: $c2, $mb: $p4})(*children)

@c
def StatusBadge(status):
    color = "#16a34a" if status == "active" else "#f59e0b" if status == "trial" else $c1
    return span(st={$dsp: $ib, $pad: "2px 8px", $brr: $p1, $bg: color, $col: $c2, $fs: $p3})(status)

@c
def PlanBadge(plan):
    color = $cE if plan == "enterprise" else $c4 if plan == "pro" else $c1
    return span(st={$dsp: $ib, $pad: "2px 8px", $brr: $p1, $bg: color, $col: $c2, $fs: $p3})(plan)

@c
def CustomerListRow(customer, on_open):
    def handle():
        on_open(customer.id)
    return tr(oc=handle, st={$cur: "pointer", $bb: $br2})(td(st={$pad: $p2})(str(customer.id)), td(st={$pad: $p2})(customer.name), td(st={$pad: $p2})(customer.email), td(st={$pad: $p2})(StatusBadge(status=customer.status)), td(st={$pad: $p2})(PlanBadge(plan=customer.plan)), td(st={$pad: $p2, $ta: "right"})("$", f"{customer.monthly_value:.2f}"), td(st={$pad: $p2})(customer.created_at))

@c
def Pagination(page, total_pages, on_change):
    def make_handler(p):
        return lambda: on_change(p)

    pages = []
    start = max(0, page - 2)
    end = min(total_pages, page + 3)
    for p in range(start, end):
        pages.append(p)

    return div(st={$dsp: "flex", "gap": "4px", $ai: "center"})(
        Button(label="Prev", oc=make_handler(max(0, page - 1)), kind="default"),
        *[Button(label=str(p + 1), oc=make_handler(p), kind="primary" if p == page else "default") for p in pages],
        Button(label="Next", oc=make_handler(min(total_pages - 1, page + 1)), kind="default"),
        span(st={"margin_left": $p1, $fs: $p1, $col: $c3})(
            "Page ", page + 1, " of ", total_pages
        ),
    )

@c
def Filters(state, on_change):
    def update(field, value):
        new_state = FilterState(
            query=state.query if field != "query" else value,
            status=state.status if field != "status" else value,
            plan=state.plan if field != "plan" else value,
            sort_by=state.sort_by if field != "sort_by" else value,
            sort_dir=state.sort_dir if field != "sort_dir" else value,
        )
        on_change(new_state)

    return Card()(
        h3(st={$mar: "0 0 12px 0", $fs: "14px"})("Filters"),
        Field(text="Search", error="")(
            TextInput(value=state.query, oh=lambda v: update("query", v), ph="name or email"),
        ),
        Field(text="Status", error="")(
            Select(value=state.status, options=["all", "active", "trial", "churned"], oh=lambda v: update("status", v)),
        ),
        Field(text="Plan", error="")(
            Select(value=state.plan, options=["all", "starter", "pro", "enterprise"], oh=lambda v: update("plan", v)),
        ),
        Field(text="Sort by", error="")(
            Select(value=state.sort_by, options=["name", "monthly_value", "created_at"], oh=lambda v: update("sort_by", v)),
        ),
        Field(text="Direction", error="")(
            Select(value=state.sort_dir, options=["asc", "desc"], oh=lambda v: update("sort_dir", v)),
        ),
    )

@c
def CustomerList(on_open, on_create):
    customers, set_customers = us([])
    total, set_total = us(0)
    page, set_page = us(0)
    page_size = 20
    filter_state, set_filter_state = us(FilterState(
        query="", status="all", plan="all", sort_by="name", sort_dir="asc"
    ))
    loading, set_loading = us(True)

    async def load():
        set_loading(True)
        result = await api_list_customers(filter_state, page, page_size)
        set_customers(result.items)
        set_total(result.total)
        set_loading(False)

    ue(lambda: load(), [filter_state, page])

    total_pages = max(1, (total + page_size - 1) // page_size)

    return div(st={$dsp: "grid", $gtc: "260px 1fr", "gap": $p4})(
        Filters(state=filter_state, oh=set_filter_state),
        div()(
            Toolbar()(
                Button(label="+ New Customer", oc=on_create, kind="primary"),
                span(st={"flex": 1})(""),
                span(st={$fs: $p1, $col: $c3})(total, " total"),
            ),
            Card()(
                table(st={$wd: $pct, "border_collapse": "collapse"})(
                    thead()(
                        tr(st={$bg: $c5, $ta: "left"})(
                            th(st={$pad: $p2})("ID"),
                            th(st={$pad: $p2})("Name"),
                            th(st={$pad: $p2})("Email"),
                            th(st={$pad: $p2})("Status"),
                            th(st={$pad: $p2})("Plan"),
                            th(st={$pad: $p2, $ta: "right"})("Monthly"),
                            th(st={$pad: $p2})("Created"),
                        ),
                    ),
                    tbody()(
                        *[CustomerListRow(customer=c, on_open=on_open) for c in customers],
                    ),
                ),
                div(st={$ta: "center", $pad: $p1, $col: $c1})(
                    "Loading..." if loading else "",
                ),
            ),
            Pagination(page=page, total_pages=total_pages, oh=set_page),
        ),
    )

@c
def ActivityList(activities):
    if len(activities) == 0:
        return div(st={$col: $c1})("No recent activity")
    return ul(st={$ls: "none", $pad: 0})(
        *[
            li(key=str(a.id), st={$pad: "8px 0", $bb: "1px solid #f3f4f6"})(
                div(st={$fw: "600"})(a.kind),
                div(st={$fs: "13px"})(a.summary),
                div(st={$fs: $p3, $col: $c1})(a.timestamp),
            )
            for a in activities
        ],
    )

@c
def CustomerDetail(customer_id, on_back, on_edit, on_delete):
    customer, set_customer = us(None)
    activities, set_activities = us([])
    loading, set_loading = us(True)

    async def load():
        set_loading(True)
        c = await api_get_customer(customer_id)
        a = await api_get_activity(customer_id)
        set_customer(c)
        set_activities(a)
        set_loading(False)

    ue(lambda: load(), [customer_id])

    if loading or customer is None:
        return div(st={$pad: "32px", $ta: "center"})("Loading...")

    return div()(
        Toolbar()(
            Button(label="â† Back", oc=on_back, kind="default"),
            Button(label="Edit", oc=lambda: on_edit(customer_id), kind="primary"),
            Button(label="Delete", oc=lambda: on_delete(customer_id), kind="danger"),
        ),
        Card()(
            h2(st={$mar: "0 0 8px 0"})(customer.name),
            div(st={$col: $c3, $mb: $p1})(customer.email),
            div(st={$dsp: "flex", "gap": $p6, $mb: $p4})(
                StatusBadge(status=customer.status),
                PlanBadge(plan=customer.plan),
            ),
            div(st={$dsp: "grid", $gtc: "1fr 1fr", "gap": $p1})(
                div()(
                    div(st={$fs: $p3, $col: $c1})("Phone"),
                    div()(customer.phone),
                ),
                div()(
                    div(st={$fs: $p3, $col: $c1})("Monthly value"),
                    div()("$", f"{customer.monthly_value:.2f}"),
                ),
                div()(
                    div(st={$fs: $p3, $col: $c1})("Created"),
                    div()(customer.created_at),
                ),
            ),
            div(st={$mt: $p4})(
                div(st={$fs: $p3, $col: $c1})("Notes"),
                div(st={$ws: $pwr})(customer.notes if customer.notes != "" else "(none)"),
            ),
        ),
        Card()(
            h3(st={$mar: "0 0 12px 0", $fs: "14px"})("Recent activity"),
            ActivityList(activities=activities),
        ),
    )

@c
def CustomerForm(customer_id, on_back, on_saved):
    is_create = customer_id is None
    name, set_name = us("")
    email, set_email = us("")
    phone, set_phone = us("")
    status, set_status = us("active")
    plan, set_plan = us("starter")
    monthly_value, set_monthly_value = us(49.0)
    notes, set_notes = us("")
    errors, set_errors = us({})
    saving, set_saving = us(False)

    async def load():
        if customer_id is not None:
            c = await api_get_customer(customer_id)
            set_name(c.name)
            set_email(c.email)
            set_phone(c.phone)
            set_status(c.status)
            set_plan(c.plan)
            set_monthly_value(c.monthly_value)
            set_notes(c.notes)

    ue(lambda: load(), [customer_id])

    async def save():
        draft = CustomerDraft(
            name=name, email=email, phone=phone, status=status,
            plan=plan, monthly_value=monthly_value, notes=notes,
        )
        validation_errors = validate_draft(draft)
        if len(validation_errors) > 0:
            set_errors(validation_errors)
            return
        set_saving(True)
        saved = await api_save_customer(draft, customer_id)
        set_saving(False)
        on_saved(saved.id)

    return div()(
        Toolbar()(
            Button(label="â† Back", oc=on_back, kind="default"),
            span(st={"flex": 1})(""),
            Button(label="Save" if not saving else "Saving...", oc=lambda: save(), kind="primary"),
        ),
        Card()(
            h2(st={$mar: "0 0 16px 0"})("New Customer" if is_create else "Edit Customer"),
            Field(text="Name", error=errors.get("name", ""))(
                TextInput(value=name, oh=set_name, ph="Acme Inc."),
            ),
            Field(text="Email", error=errors.get("email", ""))(
                TextInput(value=email, oh=set_email, ph="contact@acme.com"),
            ),
            Field(text="Phone", error=errors.get("phone", ""))(
                TextInput(value=phone, oh=set_phone, ph="+1-555-0100"),
            ),
            div(st={$dsp: "grid", $gtc: "1fr 1fr", "gap": $p1})(
                Field(text="Status", error="")(
                    Select(value=status, options=["active", "trial", "churned"], oh=set_status),
                ),
                Field(text="Plan", error="")(
                    Select(value=plan, options=["starter", "pro", "enterprise"], oh=set_plan),
                ),
            ),
            Field(text="Monthly value (USD)", error=errors.get("monthly_value", ""))(
                NumberInput(value=monthly_value, oh=set_monthly_value, ph="0.00"),
            ),
            Field(text="Notes", error="")(
                textarea(
                    value=notes,
                    oh=lambda e: set_notes(e.target.value),
                    rows=4,
                    st={$wd: $pct, $pad: "8px 10px", $bd: $br1, $brr: $p5},
                ),
            ),
        ),
    )

@c
def App():
    route, set_route = us(("list", None))

    def navigate_to(name, customer_id):
        new_route = (name, customer_id)
        set_route(new_route)

    ue(lambda: parse_initial_route(set_route), [])

    async def delete_customer(customer_id):
        ok = await api_delete_customer(customer_id)
        if ok:
            navigate_to("list", None)

    name = route[0]
    cid = route[1]

    body = None
    if name == "list":
        body = CustomerList(
            on_open=lambda c: navigate_to("detail", c),
            on_create=lambda: navigate_to("create", None),
        )
    elif name == "detail":
        body = CustomerDetail(
            customer_id=cid,
            on_back=lambda: navigate_to("list", None),
            on_edit=lambda c: navigate_to("edit", c),
            on_delete=lambda c: delete_customer(c),
        )
    elif name == "create":
        body = CustomerForm(
            customer_id=None,
            on_back=lambda: navigate_to("list", None),
            on_saved=lambda c: navigate_to("detail", c),
        )
    elif name == "edit":
        body = CustomerForm(
            customer_id=cid,
            on_back=lambda: navigate_to("detail", cid),
            on_saved=lambda c: navigate_to("detail", c),
        )

    return div(st={$ff_k: "system-ui, sans-serif", $bg: $c5, $mh: $vh})(
        header(st={
            $pad: "16px 24px",
            $bg: $c2,
            $bb: $br2,
            $dsp: "flex",
            $ai: "center",
        })(
            h1(st={$mar: 0, $fs: "20px"})("Customer CRM"),
            span(st={"flex": 1})(""),
            span(st={$fs: $p1, $col: $c3})("v0.1 · 2026-05-07"),
        ),
        main(st={$pad: $p7, $mw: "1280px", $mar: "0 auto"})(
            body,
        ),
    )

def parse_initial_route(set_route):
    set_route(("list", None))
    return None
