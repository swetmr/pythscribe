import React, { useCallback, useEffect, useMemo, useState } from "react";

// ============================================================================
// Domain models
// ============================================================================

interface Customer {
  id: number;
  name: string;
  email: string;
  phone: string;
  status: string;
  plan: string;
  monthly_value: number;
  created_at: string;
  notes: string;
}

interface CustomerDraft {
  name: string;
  email: string;
  phone: string;
  status: string;
  plan: string;
  monthly_value: number;
  notes: string;
}

interface Activity {
  id: number;
  customer_id: number;
  kind: string;
  summary: string;
  timestamp: string;
}

interface Page {
  items: Customer[];
  total: number;
  page: number;
  page_size: number;
}

interface FilterState {
  query: string;
  status: string;
  plan: string;
  sort_by: string;
  sort_dir: string;
}

// ============================================================================
// Validation helpers
// ============================================================================

function validateEmail(email: string): string {
  if (!email.includes("@")) {
    return "Email must contain @";
  }
  if (!email.split("@")[1].includes(".")) {
    return "Email domain looks invalid";
  }
  return "";
}

function validateRequired(value: string, label: string): string {
  if (value === "" || value === null || value === undefined) {
    return `${label} is required`;
  }
  return "";
}

function validatePhone(phone: string): string {
  let digits = "";
  for (const c of phone) {
    if (c >= "0" && c <= "9") {
      digits = digits + c;
    }
  }
  if (digits.length < 7) {
    return "Phone must have at least 7 digits";
  }
  return "";
}

function validateMoney(value: number): string {
  if (value < 0) {
    return "Amount must be non-negative";
  }
  if (value > 1_000_000) {
    return "Amount looks too large; double-check";
  }
  return "";
}

function validateDraft(draft: CustomerDraft): Record<string, string> {
  const errors: Record<string, string> = {};
  let e = validateRequired(draft.name, "Name");
  if (e !== "") errors.name = e;
  e = validateEmail(draft.email);
  if (e !== "") errors.email = e;
  e = validatePhone(draft.phone);
  if (e !== "") errors.phone = e;
  e = validateMoney(draft.monthly_value);
  if (e !== "") errors.monthly_value = e;
  return errors;
}

// ============================================================================
// Synthetic API
// ============================================================================

function sleep(seconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, seconds * 1000));
}

async function apiListCustomers(
  filters: FilterState,
  page: number,
  pageSize: number,
): Promise<Page> {
  await sleep(0.05);
  const base: Customer[] = [];
  for (let i = 0; i < 120; i++) {
    base.push({
      id: i + 1,
      name: `Customer ${i + 1}`,
      email: `c${i + 1}@example.com`,
      phone: `+1-555-${String(1000 + i).padStart(4, "0")}`,
      status: i % 3 !== 0 ? "active" : i % 3 === 1 ? "trial" : "churned",
      plan: i % 5 === 0 ? "enterprise" : i % 2 === 0 ? "pro" : "starter",
      monthly_value: 49.0 + (i % 11) * 50.0,
      created_at: `2026-${String((i % 12) + 1).padStart(2, "0")}-${String((i % 28) + 1).padStart(2, "0")}`,
      notes: "",
    });
  }
  let filtered: Customer[] = [];
  const q = filters.query.toLowerCase();
  for (const c of base) {
    if (filters.status !== "all" && c.status !== filters.status) continue;
    if (filters.plan !== "all" && c.plan !== filters.plan) continue;
    if (q !== "") {
      if (
        !c.name.toLowerCase().includes(q) &&
        !c.email.toLowerCase().includes(q)
      ) {
        continue;
      }
    }
    filtered.push(c);
  }

  if (filters.sort_by === "name") {
    filtered = filtered.slice().sort((a, b) => a.name.localeCompare(b.name));
  } else if (filters.sort_by === "monthly_value") {
    filtered = filtered.slice().sort((a, b) => a.monthly_value - b.monthly_value);
  } else if (filters.sort_by === "created_at") {
    filtered = filtered
      .slice()
      .sort((a, b) => a.created_at.localeCompare(b.created_at));
  }

  if (filters.sort_dir === "desc") {
    filtered = filtered.slice().reverse();
  }

  const start = page * pageSize;
  const end = start + pageSize;
  return {
    items: filtered.slice(start, end),
    total: filtered.length,
    page,
    page_size: pageSize,
  };
}

async function apiGetCustomer(customerId: number): Promise<Customer> {
  await sleep(0.03);
  return {
    id: customerId,
    name: `Customer ${customerId}`,
    email: `c${customerId}@example.com`,
    phone: `+1-555-${String(1000 + customerId - 1).padStart(4, "0")}`,
    status: "active",
    plan: "pro",
    monthly_value: 149.0,
    created_at: "2026-01-15",
    notes: "Long-time customer; renewal upcoming.",
  };
}

async function apiSaveCustomer(
  draft: CustomerDraft,
  existingId: number | null,
): Promise<Customer> {
  await sleep(0.08);
  const newId = existingId !== null ? existingId : 99999;
  return {
    id: newId,
    name: draft.name,
    email: draft.email,
    phone: draft.phone,
    status: draft.status,
    plan: draft.plan,
    monthly_value: draft.monthly_value,
    created_at: "2026-05-07",
    notes: draft.notes,
  };
}

async function apiDeleteCustomer(_customerId: number): Promise<boolean> {
  await sleep(0.04);
  return true;
}

async function apiGetActivity(customerId: number): Promise<Activity[]> {
  await sleep(0.04);
  return [
    {
      id: 1,
      customer_id: customerId,
      kind: "login",
      summary: "Logged in",
      timestamp: "14:23",
    },
    {
      id: 2,
      customer_id: customerId,
      kind: "upgrade",
      summary: "Upgraded to pro",
      timestamp: "13:10",
    },
    {
      id: 3,
      customer_id: customerId,
      kind: "payment",
      summary: "Paid invoice #5021",
      timestamp: "2026-05-01",
    },
  ];
}

// ============================================================================
// Routing helpers
// ============================================================================

type Route = ["list", null] | ["create", null] | ["detail", number] | ["edit", number];

function parseRoute(hashStr: string): Route {
  if (hashStr === "" || hashStr === "#") {
    return ["list", null];
  }
  const h = hashStr.replace(/^#/, "").replace(/^\//, "");
  const parts = h.split("/");
  if (parts[0] === "customers") {
    if (parts.length === 1) return ["list", null];
    if (parts.length === 2 && parts[1] === "new") return ["create", null];
    if (parts.length === 2) {
      const cid = parts[1];
      if (/^\d+$/.test(cid)) return ["detail", parseInt(cid, 10)];
    }
    if (parts.length === 3 && parts[2] === "edit") {
      const cid = parts[1];
      if (/^\d+$/.test(cid)) return ["edit", parseInt(cid, 10)];
    }
  }
  return ["list", null];
}

function routeToHash(name: string, customerId: number | null): string {
  if (name === "list") return "#/customers";
  if (name === "create") return "#/customers/new";
  if (name === "detail") return `#/customers/${customerId}`;
  if (name === "edit") return `#/customers/${customerId}/edit`;
  return "#/customers";
}

// ============================================================================
// UI primitives
// ============================================================================

interface FieldProps {
  label: string;
  error: string;
  children: React.ReactNode;
}

function Field({ label, error, children }: FieldProps): JSX.Element {
  const errorColor = "#dc2626";
  return (
    <div style={{ marginBottom: "12px" }}>
      <label
        style={{
          display: "block",
          fontSize: "12px",
          color: "#374151",
          marginBottom: "4px",
        }}
      >
        {label}
      </label>
      {children}
      <div
        style={{
          fontSize: "11px",
          color: errorColor,
          marginTop: "4px",
          minHeight: "14px",
        }}
      >
        {error !== "" ? error : ""}
      </div>
    </div>
  );
}

interface TextInputProps {
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
}

function TextInput({ value, onChange, placeholder }: TextInputProps): JSX.Element {
  return (
    <input
      type="text"
      value={value}
      placeholder={placeholder}
      onChange={(e) => onChange(e.target.value)}
      style={{
        width: "100%",
        padding: "8px 10px",
        border: "1px solid #d1d5db",
        borderRadius: "6px",
      }}
    />
  );
}

interface NumberInputProps {
  value: number;
  onChange: (v: number) => void;
  placeholder: string;
}

function NumberInput({ value, onChange, placeholder }: NumberInputProps): JSX.Element {
  return (
    <input
      type="number"
      value={value}
      placeholder={placeholder}
      onChange={(e) => {
        const v = parseFloat(e.target.value);
        onChange(isNaN(v) ? 0.0 : v);
      }}
      style={{
        width: "100%",
        padding: "8px 10px",
        border: "1px solid #d1d5db",
        borderRadius: "6px",
      }}
    />
  );
}

interface SelectProps {
  value: string;
  options: string[];
  onChange: (v: string) => void;
}

function Select({ value, options, onChange }: SelectProps): JSX.Element {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      style={{
        width: "100%",
        padding: "8px 10px",
        border: "1px solid #d1d5db",
        borderRadius: "6px",
      }}
    >
      {options.map((o) => (
        <option key={o} value={o}>
          {o}
        </option>
      ))}
    </select>
  );
}

interface ButtonProps {
  label: string;
  onClick: () => void;
  kind: string;
}

function Button({ label, onClick, kind }: ButtonProps): JSX.Element {
  const bg =
    kind === "primary" ? "#3b82f6" : kind === "default" ? "#ffffff" : "#dc2626";
  const color = kind !== "default" ? "#ffffff" : "#374151";
  return (
    <button
      onClick={onClick}
      style={{
        padding: "8px 16px",
        background: bg,
        color: color,
        border: "1px solid #d1d5db",
        borderRadius: "6px",
        cursor: "pointer",
      }}
    >
      {label}
    </button>
  );
}

interface ToolbarProps {
  children: React.ReactNode;
}

function Toolbar({ children }: ToolbarProps): JSX.Element {
  return (
    <div style={{ display: "flex", gap: "8px", padding: "12px 0" }}>
      {children}
    </div>
  );
}

interface CardProps {
  children: React.ReactNode;
}

function Card({ children }: CardProps): JSX.Element {
  return (
    <div
      style={{
        padding: "16px",
        border: "1px solid #e5e7eb",
        borderRadius: "8px",
        background: "#ffffff",
        marginBottom: "16px",
      }}
    >
      {children}
    </div>
  );
}

interface StatusBadgeProps {
  status: string;
}

function StatusBadge({ status }: StatusBadgeProps): JSX.Element {
  const color =
    status === "active" ? "#16a34a" : status === "trial" ? "#f59e0b" : "#9ca3af";
  return (
    <span
      style={{
        display: "inline-block",
        padding: "2px 8px",
        borderRadius: "12px",
        background: color,
        color: "#ffffff",
        fontSize: "11px",
      }}
    >
      {status}
    </span>
  );
}

interface PlanBadgeProps {
  plan: string;
}

function PlanBadge({ plan }: PlanBadgeProps): JSX.Element {
  const color =
    plan === "enterprise" ? "#7c3aed" : plan === "pro" ? "#3b82f6" : "#9ca3af";
  return (
    <span
      style={{
        display: "inline-block",
        padding: "2px 8px",
        borderRadius: "12px",
        background: color,
        color: "#ffffff",
        fontSize: "11px",
      }}
    >
      {plan}
    </span>
  );
}

// ============================================================================
// Customer list view
// ============================================================================

interface CustomerListRowProps {
  customer: Customer;
  onOpen: (id: number) => void;
}

function CustomerListRow({ customer, onOpen }: CustomerListRowProps): JSX.Element {
  return (
    <tr
      onClick={() => onOpen(customer.id)}
      style={{ cursor: "pointer", borderBottom: "1px solid #e5e7eb" }}
    >
      <td style={{ padding: "10px" }}>{customer.id}</td>
      <td style={{ padding: "10px" }}>{customer.name}</td>
      <td style={{ padding: "10px" }}>{customer.email}</td>
      <td style={{ padding: "10px" }}>
        <StatusBadge status={customer.status} />
      </td>
      <td style={{ padding: "10px" }}>
        <PlanBadge plan={customer.plan} />
      </td>
      <td style={{ padding: "10px", textAlign: "right" }}>
        ${customer.monthly_value.toFixed(2)}
      </td>
      <td style={{ padding: "10px" }}>{customer.created_at}</td>
    </tr>
  );
}

interface PaginationProps {
  page: number;
  totalPages: number;
  onChange: (p: number) => void;
}

function Pagination({ page, totalPages, onChange }: PaginationProps): JSX.Element {
  const pages: number[] = [];
  const start = Math.max(0, page - 2);
  const end = Math.min(totalPages, page + 3);
  for (let p = start; p < end; p++) {
    pages.push(p);
  }

  return (
    <div style={{ display: "flex", gap: "4px", alignItems: "center" }}>
      <Button
        label="Prev"
        onClick={() => onChange(Math.max(0, page - 1))}
        kind="default"
      />
      {pages.map((p) => (
        <Button
          key={p}
          label={String(p + 1)}
          onClick={() => onChange(p)}
          kind={p === page ? "primary" : "default"}
        />
      ))}
      <Button
        label="Next"
        onClick={() => onChange(Math.min(totalPages - 1, page + 1))}
        kind="default"
      />
      <span style={{ marginLeft: "12px", fontSize: "12px", color: "#6b7280" }}>
        Page {page + 1} of {totalPages}
      </span>
    </div>
  );
}

interface FiltersProps {
  state: FilterState;
  onChange: (s: FilterState) => void;
}

function Filters({ state, onChange }: FiltersProps): JSX.Element {
  const update = (field: keyof FilterState, value: string) => {
    onChange({ ...state, [field]: value });
  };

  return (
    <Card>
      <h3 style={{ margin: "0 0 12px 0", fontSize: "14px" }}>Filters</h3>
      <Field label="Search" error="">
        <TextInput
          value={state.query}
          onChange={(v) => update("query", v)}
          placeholder="name or email"
        />
      </Field>
      <Field label="Status" error="">
        <Select
          value={state.status}
          options={["all", "active", "trial", "churned"]}
          onChange={(v) => update("status", v)}
        />
      </Field>
      <Field label="Plan" error="">
        <Select
          value={state.plan}
          options={["all", "starter", "pro", "enterprise"]}
          onChange={(v) => update("plan", v)}
        />
      </Field>
      <Field label="Sort by" error="">
        <Select
          value={state.sort_by}
          options={["name", "monthly_value", "created_at"]}
          onChange={(v) => update("sort_by", v)}
        />
      </Field>
      <Field label="Direction" error="">
        <Select
          value={state.sort_dir}
          options={["asc", "desc"]}
          onChange={(v) => update("sort_dir", v)}
        />
      </Field>
    </Card>
  );
}

interface CustomerListProps {
  onOpen: (id: number) => void;
  onCreate: () => void;
}

function CustomerList({ onOpen, onCreate }: CustomerListProps): JSX.Element {
  const [customers, setCustomers] = useState<Customer[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(0);
  const pageSize = 20;
  const [filterState, setFilterState] = useState<FilterState>({
    query: "",
    status: "all",
    plan: "all",
    sort_by: "name",
    sort_dir: "asc",
  });
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    const result = await apiListCustomers(filterState, page, pageSize);
    setCustomers(result.items);
    setTotal(result.total);
    setLoading(false);
  }, [filterState, page]);

  useEffect(() => {
    load();
  }, [load]);

  const totalPages = Math.max(1, Math.ceil(total / pageSize));

  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "260px 1fr",
        gap: "16px",
      }}
    >
      <Filters state={filterState} onChange={setFilterState} />
      <div>
        <Toolbar>
          <Button label="+ New Customer" onClick={onCreate} kind="primary" />
          <span style={{ flex: 1 }}></span>
          <span style={{ fontSize: "12px", color: "#6b7280" }}>
            {total} total
          </span>
        </Toolbar>
        <Card>
          <table style={{ width: "100%", borderCollapse: "collapse" }}>
            <thead>
              <tr style={{ background: "#f9fafb", textAlign: "left" }}>
                <th style={{ padding: "10px" }}>ID</th>
                <th style={{ padding: "10px" }}>Name</th>
                <th style={{ padding: "10px" }}>Email</th>
                <th style={{ padding: "10px" }}>Status</th>
                <th style={{ padding: "10px" }}>Plan</th>
                <th style={{ padding: "10px", textAlign: "right" }}>Monthly</th>
                <th style={{ padding: "10px" }}>Created</th>
              </tr>
            </thead>
            <tbody>
              {customers.map((c) => (
                <CustomerListRow key={c.id} customer={c} onOpen={onOpen} />
              ))}
            </tbody>
          </table>
          <div style={{ textAlign: "center", padding: "12px", color: "#9ca3af" }}>
            {loading ? "Loading..." : ""}
          </div>
        </Card>
        <Pagination page={page} totalPages={totalPages} onChange={setPage} />
      </div>
    </div>
  );
}

// ============================================================================
// Customer detail view
// ============================================================================

interface ActivityListProps {
  activities: Activity[];
}

function ActivityList({ activities }: ActivityListProps): JSX.Element {
  if (activities.length === 0) {
    return <div style={{ color: "#9ca3af" }}>No recent activity</div>;
  }
  return (
    <ul style={{ listStyle: "none", padding: 0 }}>
      {activities.map((a) => (
        <li
          key={a.id}
          style={{ padding: "8px 0", borderBottom: "1px solid #f3f4f6" }}
        >
          <div style={{ fontWeight: "600" }}>{a.kind}</div>
          <div style={{ fontSize: "13px" }}>{a.summary}</div>
          <div style={{ fontSize: "11px", color: "#9ca3af" }}>
            {a.timestamp}
          </div>
        </li>
      ))}
    </ul>
  );
}

interface CustomerDetailProps {
  customerId: number;
  onBack: () => void;
  onEdit: (id: number) => void;
  onDelete: (id: number) => void;
}

function CustomerDetail({
  customerId,
  onBack,
  onEdit,
  onDelete,
}: CustomerDetailProps): JSX.Element {
  const [customer, setCustomer] = useState<Customer | null>(null);
  const [activities, setActivities] = useState<Activity[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const load = async () => {
      setLoading(true);
      const c = await apiGetCustomer(customerId);
      const a = await apiGetActivity(customerId);
      setCustomer(c);
      setActivities(a);
      setLoading(false);
    };
    load();
  }, [customerId]);

  if (loading || customer === null) {
    return (
      <div style={{ padding: "32px", textAlign: "center" }}>Loading...</div>
    );
  }

  return (
    <div>
      <Toolbar>
        <Button label="← Back" onClick={onBack} kind="default" />
        <Button
          label="Edit"
          onClick={() => onEdit(customerId)}
          kind="primary"
        />
        <Button
          label="Delete"
          onClick={() => onDelete(customerId)}
          kind="danger"
        />
      </Toolbar>
      <Card>
        <h2 style={{ margin: "0 0 8px 0" }}>{customer.name}</h2>
        <div style={{ color: "#6b7280", marginBottom: "12px" }}>
          {customer.email}
        </div>
        <div
          style={{ display: "flex", gap: "8px", marginBottom: "16px" }}
        >
          <StatusBadge status={customer.status} />
          <PlanBadge plan={customer.plan} />
        </div>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 1fr",
            gap: "12px",
          }}
        >
          <div>
            <div style={{ fontSize: "11px", color: "#9ca3af" }}>Phone</div>
            <div>{customer.phone}</div>
          </div>
          <div>
            <div style={{ fontSize: "11px", color: "#9ca3af" }}>
              Monthly value
            </div>
            <div>${customer.monthly_value.toFixed(2)}</div>
          </div>
          <div>
            <div style={{ fontSize: "11px", color: "#9ca3af" }}>Created</div>
            <div>{customer.created_at}</div>
          </div>
        </div>
        <div style={{ marginTop: "16px" }}>
          <div style={{ fontSize: "11px", color: "#9ca3af" }}>Notes</div>
          <div style={{ whiteSpace: "pre-wrap" }}>
            {customer.notes !== "" ? customer.notes : "(none)"}
          </div>
        </div>
      </Card>
      <Card>
        <h3 style={{ margin: "0 0 12px 0", fontSize: "14px" }}>
          Recent activity
        </h3>
        <ActivityList activities={activities} />
      </Card>
    </div>
  );
}

// ============================================================================
// Customer edit / create form
// ============================================================================

interface CustomerFormProps {
  customerId: number | null;
  onBack: () => void;
  onSaved: (id: number) => void;
}

function CustomerForm({
  customerId,
  onBack,
  onSaved,
}: CustomerFormProps): JSX.Element {
  const isCreate = customerId === null;
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [phone, setPhone] = useState("");
  const [status, setStatus] = useState("active");
  const [plan, setPlan] = useState("starter");
  const [monthlyValue, setMonthlyValue] = useState(49.0);
  const [notes, setNotes] = useState("");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    const load = async () => {
      if (customerId !== null) {
        const c = await apiGetCustomer(customerId);
        setName(c.name);
        setEmail(c.email);
        setPhone(c.phone);
        setStatus(c.status);
        setPlan(c.plan);
        setMonthlyValue(c.monthly_value);
        setNotes(c.notes);
      }
    };
    load();
  }, [customerId]);

  const save = async () => {
    const draft: CustomerDraft = {
      name,
      email,
      phone,
      status,
      plan,
      monthly_value: monthlyValue,
      notes,
    };
    const validationErrors = validateDraft(draft);
    if (Object.keys(validationErrors).length > 0) {
      setErrors(validationErrors);
      return;
    }
    setSaving(true);
    const saved = await apiSaveCustomer(draft, customerId);
    setSaving(false);
    onSaved(saved.id);
  };

  return (
    <div>
      <Toolbar>
        <Button label="← Back" onClick={onBack} kind="default" />
        <span style={{ flex: 1 }}></span>
        <Button
          label={saving ? "Saving..." : "Save"}
          onClick={save}
          kind="primary"
        />
      </Toolbar>
      <Card>
        <h2 style={{ margin: "0 0 16px 0" }}>
          {isCreate ? "New Customer" : "Edit Customer"}
        </h2>
        <Field label="Name" error={errors.name || ""}>
          <TextInput
            value={name}
            onChange={setName}
            placeholder="Acme Inc."
          />
        </Field>
        <Field label="Email" error={errors.email || ""}>
          <TextInput
            value={email}
            onChange={setEmail}
            placeholder="contact@acme.com"
          />
        </Field>
        <Field label="Phone" error={errors.phone || ""}>
          <TextInput
            value={phone}
            onChange={setPhone}
            placeholder="+1-555-0100"
          />
        </Field>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 1fr",
            gap: "12px",
          }}
        >
          <Field label="Status" error="">
            <Select
              value={status}
              options={["active", "trial", "churned"]}
              onChange={setStatus}
            />
          </Field>
          <Field label="Plan" error="">
            <Select
              value={plan}
              options={["starter", "pro", "enterprise"]}
              onChange={setPlan}
            />
          </Field>
        </div>
        <Field
          label="Monthly value (USD)"
          error={errors.monthly_value || ""}
        >
          <NumberInput
            value={monthlyValue}
            onChange={setMonthlyValue}
            placeholder="0.00"
          />
        </Field>
        <Field label="Notes" error="">
          <textarea
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            rows={4}
            style={{
              width: "100%",
              padding: "8px 10px",
              border: "1px solid #d1d5db",
              borderRadius: "6px",
            }}
          />
        </Field>
      </Card>
    </div>
  );
}

// ============================================================================
// Top-level App
// ============================================================================

type AppRoute =
  | { name: "list" }
  | { name: "create" }
  | { name: "detail"; customerId: number }
  | { name: "edit"; customerId: number };

export function App(): JSX.Element {
  const [route, setRoute] = useState<AppRoute>({ name: "list" });

  const navigateTo = useCallback(
    (name: string, customerId: number | null) => {
      let newRoute: AppRoute;
      if (name === "list") newRoute = { name: "list" };
      else if (name === "create") newRoute = { name: "create" };
      else if (name === "detail")
        newRoute = { name: "detail", customerId: customerId! };
      else newRoute = { name: "edit", customerId: customerId! };
      setRoute(newRoute);
    },
    [],
  );

  useEffect(() => {
    setRoute({ name: "list" });
  }, []);

  const deleteCustomer = useCallback(
    async (customerId: number) => {
      const ok = await apiDeleteCustomer(customerId);
      if (ok) {
        navigateTo("list", null);
      }
    },
    [navigateTo],
  );

  let body: JSX.Element;
  if (route.name === "list") {
    body = (
      <CustomerList
        onOpen={(c) => navigateTo("detail", c)}
        onCreate={() => navigateTo("create", null)}
      />
    );
  } else if (route.name === "detail") {
    body = (
      <CustomerDetail
        customerId={route.customerId}
        onBack={() => navigateTo("list", null)}
        onEdit={(c) => navigateTo("edit", c)}
        onDelete={(c) => deleteCustomer(c)}
      />
    );
  } else if (route.name === "create") {
    body = (
      <CustomerForm
        customerId={null}
        onBack={() => navigateTo("list", null)}
        onSaved={(c) => navigateTo("detail", c)}
      />
    );
  } else {
    body = (
      <CustomerForm
        customerId={route.customerId}
        onBack={() => navigateTo("detail", route.customerId)}
        onSaved={(c) => navigateTo("detail", c)}
      />
    );
  }

  return (
    <div
      style={{
        fontFamily: "system-ui, sans-serif",
        background: "#f9fafb",
        minHeight: "100vh",
      }}
    >
      <header
        style={{
          padding: "16px 24px",
          background: "#ffffff",
          borderBottom: "1px solid #e5e7eb",
          display: "flex",
          alignItems: "center",
        }}
      >
        <h1 style={{ margin: 0, fontSize: "20px" }}>Customer CRM</h1>
        <span style={{ flex: 1 }}></span>
        <span style={{ fontSize: "12px", color: "#6b7280" }}>
          v0.1 · 2026-05-07
        </span>
      </header>
      <main
        style={{
          padding: "24px",
          maxWidth: "1280px",
          margin: "0 auto",
        }}
      >
        {body}
      </main>
    </div>
  );
}
