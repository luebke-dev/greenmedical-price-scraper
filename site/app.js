const columns = [
  { key: "name", label: "Sorte", type: "text", width: "20%" },
  { key: "bezeichnung", label: "Bezeichnung", type: "text", width: "19%" },
  { key: "price", label: "ab €/g", type: "number", className: "price", width: "11%" },
  { key: "price_per_thc_gram", label: "ab €/g THC", type: "number", className: "price", width: "12%" },
  { key: "thc", label: "THC", type: "number", width: "8%" },
  { key: "cbd", label: "CBD", type: "number", width: "8%" },
  { key: "genetik", label: "Genetik", type: "text", width: "12%" },
  { key: "pharmacy_count", label: "Apotheken", type: "number", width: "10%" }
];

const rangeFilters = [
  { key: "price", label: "Preis", unit: " €/g", step: 0.1, decimals: 2, get: (r) => r.sort.price },
  { key: "thc", label: "THC", unit: " %", step: 0.5, decimals: 1, get: (r) => r.sort.thc },
  { key: "cbd", label: "CBD", unit: " %", step: 0.1, decimals: 1, get: (r) => r.sort.cbd }
];

const state = {
  rows: [],
  filtered: [],
  sortKey: "price",
  sortDirection: "asc",
  filters: {},
  genetics: new Set()
};

const elements = {
  head: document.getElementById("tableHead"),
  body: document.getElementById("tableBody"),
  filters: document.getElementById("filters"),
  search: document.getElementById("searchInput"),
  clear: document.getElementById("clearSearch"),
  resultCount: document.getElementById("resultCount"),
  updatedAt: document.getElementById("updatedAt"),
  toggleFilters: document.getElementById("toggleFilters"),
  metrics: document.getElementById("metrics")
};

const collator = new Intl.Collator("de", { numeric: true, sensitivity: "base" });
const priceFormatter = new Intl.NumberFormat("de-DE", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2
});

function numberFormatter(decimals) {
  return new Intl.NumberFormat("de-DE", {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals
  });
}

function getSortValue(row, key) {
  if (key === "price") return row.sort.price;
  if (key === "price_per_thc_gram") return row.sort.price_per_thc_gram;
  if (key === "thc") return row.sort.thc;
  if (key === "cbd") return row.sort.cbd;
  if (key === "pharmacy_count") return row.pharmacy_count;
  return row[key] || "";
}

function formatEuro(value, suffix) {
  if (value === null || value === undefined || Number.isNaN(value)) return "";
  return `${priceFormatter.format(value)} ${suffix}`;
}

function updateHeader() {
  elements.head.replaceChildren();

  for (const column of columns) {
    const th = document.createElement("th");
    th.style.width = column.width;
    th.setAttribute(
      "aria-sort",
      state.sortKey === column.key
        ? (state.sortDirection === "asc" ? "ascending" : "descending")
        : "none"
    );

    const button = document.createElement("button");
    button.type = "button";
    button.dataset.key = column.key;

    const label = document.createElement("span");
    label.textContent = column.label;
    button.appendChild(label);

    const indicator = document.createElement("span");
    indicator.className = "sort-indicator";
    indicator.textContent = state.sortKey === column.key
      ? (state.sortDirection === "asc" ? "^" : "v")
      : "";
    button.appendChild(indicator);

    button.addEventListener("click", () => {
      if (state.sortKey === column.key) {
        state.sortDirection = state.sortDirection === "asc" ? "desc" : "asc";
      } else {
        state.sortKey = column.key;
        state.sortDirection = "asc";
      }
      applyFilters();
    });

    th.appendChild(button);
    elements.head.appendChild(th);
  }
}

function sortRows(rows) {
  const column = columns.find((item) => item.key === state.sortKey);
  const direction = state.sortDirection === "asc" ? 1 : -1;

  rows.sort((left, right) => {
    const leftValue = getSortValue(left, state.sortKey);
    const rightValue = getSortValue(right, state.sortKey);

    if (column && column.type === "number") {
      const leftNumber = leftValue === null ? Number.POSITIVE_INFINITY : leftValue;
      const rightNumber = rightValue === null ? Number.POSITIVE_INFINITY : rightValue;
      return (leftNumber - rightNumber) * direction;
    }

    return collator.compare(String(leftValue), String(rightValue)) * direction;
  });
}

function matchesRangeFilters(row) {
  for (const config of rangeFilters) {
    const filter = state.filters[config.key];
    if (!filter) continue;
    const value = config.get(row);
    const atFullRange = filter.lo === filter.min && filter.hi === filter.max;
    if (value === null || value === undefined) {
      // Keep strains without a value only while the filter is untouched.
      if (!atFullRange) return false;
      continue;
    }
    if (value < filter.lo || value > filter.hi) return false;
  }
  return true;
}

function matchesGenetik(row) {
  if (state.genetics.size === 0) return true;
  return state.genetics.has((row.genetik || "").toLowerCase());
}

function applyFilters() {
  const query = elements.search.value.trim().toLowerCase();
  state.filtered = state.rows.filter(
    (row) =>
      (!query || row.search.includes(query)) &&
      matchesRangeFilters(row) &&
      matchesGenetik(row)
  );

  sortRows(state.filtered);
  updateHeader();
  renderRows();
  updateResultCount();
}

function updateRangeUI(config) {
  const filter = state.filters[config.key];
  const span = filter.max - filter.min || 1;
  const loPct = ((filter.lo - filter.min) / span) * 100;
  const hiPct = ((filter.hi - filter.min) / span) * 100;
  filter.fill.style.left = `${loPct}%`;
  filter.fill.style.right = `${100 - hiPct}%`;

  const fmt = numberFormatter(config.decimals);
  filter.valueEl.textContent =
    `${fmt.format(filter.lo)}${config.unit} – ${fmt.format(filter.hi)}${config.unit}`;
}

function buildRangeFilter(config) {
  const values = state.rows
    .map((row) => config.get(row))
    .filter((value) => value !== null && value !== undefined);
  if (values.length === 0) return;

  const min = Math.floor(Math.min(...values) / config.step) * config.step;
  const max = Math.ceil(Math.max(...values) / config.step) * config.step;
  if (min === max) return;

  const card = document.createElement("div");
  card.className = "filter";

  const head = document.createElement("div");
  head.className = "filter-head";
  const label = document.createElement("span");
  label.className = "filter-label";
  label.textContent = config.label;
  const valueEl = document.createElement("span");
  valueEl.className = "filter-value";
  head.append(label, valueEl);

  const range = document.createElement("div");
  range.className = "range";
  const track = document.createElement("div");
  track.className = "range-track";
  const fill = document.createElement("div");
  fill.className = "range-fill";
  track.appendChild(fill);

  const lower = document.createElement("input");
  const upper = document.createElement("input");
  for (const input of [lower, upper]) {
    input.type = "range";
    input.min = min;
    input.max = max;
    input.step = config.step;
    input.setAttribute("aria-label", `${config.label} ${input === lower ? "Minimum" : "Maximum"}`);
  }
  lower.value = min;
  upper.value = max;
  range.append(track, lower, upper);

  card.append(head, range);
  elements.filters.appendChild(card);

  const filter = { min, max, lo: min, hi: max, fill, valueEl };
  state.filters[config.key] = filter;

  lower.addEventListener("input", () => {
    filter.lo = Math.min(Number(lower.value), filter.hi);
    lower.value = filter.lo;
    updateRangeUI(config);
    applyFilters();
  });
  upper.addEventListener("input", () => {
    filter.hi = Math.max(Number(upper.value), filter.lo);
    upper.value = filter.hi;
    updateRangeUI(config);
    applyFilters();
  });

  filter.reset = () => {
    filter.lo = min;
    filter.hi = max;
    lower.value = min;
    upper.value = max;
    updateRangeUI(config);
  };

  updateRangeUI(config);
}

function buildGenetikFilter() {
  const byKey = new Map();
  for (const row of state.rows) {
    const label = row.genetik || "";
    if (label) byKey.set(label.toLowerCase(), label);
  }
  if (byKey.size < 2) return;

  const card = document.createElement("div");
  card.className = "filter filter-genetik";

  const head = document.createElement("div");
  head.className = "filter-head";
  const label = document.createElement("span");
  label.className = "filter-label";
  label.textContent = "Genetik";
  head.appendChild(label);

  const chips = document.createElement("div");
  chips.className = "chips";

  const entries = [...byKey.entries()].sort((a, b) => collator.compare(a[1], b[1]));
  for (const [key, display] of entries) {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "chip";
    chip.textContent = display;
    chip.setAttribute("aria-pressed", "false");
    chip.addEventListener("click", () => {
      const active = state.genetics.has(key);
      if (active) state.genetics.delete(key);
      else state.genetics.add(key);
      chip.classList.toggle("active", !active);
      chip.setAttribute("aria-pressed", String(!active));
      applyFilters();
    });
    chips.appendChild(chip);
  }

  card.append(head, chips);
  elements.filters.appendChild(card);
}

function initFilters() {
  elements.filters.replaceChildren();
  state.filters = {};
  state.genetics = new Set();
  buildGenetikFilter();
  for (const config of rangeFilters) buildRangeFilter(config);
}

function resetFilters() {
  state.genetics.clear();
  for (const chip of elements.filters.querySelectorAll(".chip.active")) {
    chip.classList.remove("active");
    chip.setAttribute("aria-pressed", "false");
  }
  for (const config of rangeFilters) {
    const filter = state.filters[config.key];
    if (filter) filter.reset();
  }
}

function statusBadge(value) {
  const status = document.createElement("span");
  status.className = value.toLowerCase().includes("neu") ? "status new" : "status";
  status.textContent = value;
  return status;
}

function buildOffersTable(strain) {
  const table = document.createElement("table");
  table.className = "offers";

  const thead = document.createElement("thead");
  const headRow = document.createElement("tr");
  for (const label of ["Apotheke", "Stadt", "€/g", "€/g THC", "Status", ""]) {
    const th = document.createElement("th");
    th.textContent = label;
    headRow.appendChild(th);
  }
  thead.appendChild(headRow);
  table.appendChild(thead);

  const tbody = document.createElement("tbody");
  for (const offer of strain.offers) {
    const row = document.createElement("tr");

    const apotheke = document.createElement("td");
    apotheke.textContent = offer.apotheke || "";
    row.appendChild(apotheke);

    const stadt = document.createElement("td");
    stadt.textContent = offer.apotheke_stadt || "";
    row.appendChild(stadt);

    const price = document.createElement("td");
    price.className = "price";
    price.textContent = offer.preis_pro_gramm || formatEuro(offer.preis_eur_pro_gramm, "€/g");
    row.appendChild(price);

    const thcPrice = document.createElement("td");
    thcPrice.className = "price";
    thcPrice.textContent = formatEuro(offer.preis_eur_pro_gramm_thc, "€/g THC");
    row.appendChild(thcPrice);

    const status = document.createElement("td");
    if (offer.verfuegbarkeit) status.appendChild(statusBadge(offer.verfuegbarkeit));
    row.appendChild(status);

    const buy = document.createElement("td");
    buy.className = "buy-cell";
    if (offer.produkt_url) buy.appendChild(buyLink(offer.produkt_url));
    row.appendChild(buy);

    tbody.appendChild(row);
  }
  table.appendChild(tbody);
  return table;
}

function buyLink(url) {
  const link = document.createElement("a");
  link.className = "buy";
  link.href = url;
  link.target = "_blank";
  link.rel = "noopener";
  link.textContent = "Kaufen";
  return link;
}

function renderRows() {
  elements.body.replaceChildren();

  if (state.filtered.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.className = "empty";
    cell.colSpan = columns.length;
    cell.textContent = "Keine Sorten gefunden.";
    row.appendChild(cell);
    elements.body.appendChild(row);
    return;
  }

  const fragment = document.createDocumentFragment();

  for (const strain of state.filtered) {
    const groupRow = document.createElement("tr");
    groupRow.className = "group-row";

    for (const column of columns) {
      const cell = document.createElement("td");
      if (column.className) cell.className = column.className;

      if (column.key === "name") {
        const wrap = document.createElement("span");
        wrap.className = "strain-name";
        const chevron = document.createElement("span");
        chevron.className = "chevron";
        chevron.textContent = "▸";
        wrap.appendChild(chevron);
        const text = document.createElement("span");
        text.textContent = strain.name || "—";
        wrap.appendChild(text);
        cell.appendChild(wrap);
      } else if (column.key === "price") {
        cell.textContent =
          strain.min_price === null ? "" : `ab ${formatEuro(strain.min_price, "€/g")}`;
      } else if (column.key === "price_per_thc_gram") {
        cell.textContent =
          strain.min_price_per_thc_gram === null
            ? ""
            : `ab ${formatEuro(strain.min_price_per_thc_gram, "€/g THC")}`;
      } else if (column.key === "pharmacy_count") {
        cell.textContent = strain.pharmacy_count.toLocaleString("de-DE");
      } else {
        cell.textContent = strain[column.key] || "";
      }

      groupRow.appendChild(cell);
    }

    const detailRow = document.createElement("tr");
    detailRow.className = "detail-row";
    detailRow.hidden = true;
    const detailCell = document.createElement("td");
    detailCell.colSpan = columns.length;
    detailCell.appendChild(buildOffersTable(strain));
    detailRow.appendChild(detailCell);

    groupRow.addEventListener("click", () => {
      const show = detailRow.hidden;
      detailRow.hidden = !show;
      groupRow.classList.toggle("expanded", show);
    });

    fragment.appendChild(groupRow);
    fragment.appendChild(detailRow);
  }

  elements.body.appendChild(fragment);
}

function updateResultCount() {
  const count = state.filtered.length;
  elements.resultCount.textContent = `${count.toLocaleString("de-DE")} Sorten`;
}

function metaLineEl(text, className) {
  const el = document.createElement("div");
  if (className) el.className = className;
  el.textContent = text;
  return el;
}

function fillMeta(metaEl, entry) {
  metaEl.replaceChildren();
  if (!entry) return;
  if (entry.name) metaEl.appendChild(metaLineEl(entry.name, "meta-name"));
  const facts = [
    entry.genetik,
    entry.thc && `THC ${entry.thc}`,
    entry.cbd && `CBD ${entry.cbd}`
  ].filter(Boolean).join(" · ");
  if (facts) metaEl.appendChild(metaLineEl(facts));
  if (entry.apotheke) metaEl.appendChild(metaLineEl(entry.apotheke));
}

// One entry per metric card, rendered in order into #metrics. Plain count
// metrics define `value(metadata)`; highlight metrics define `entry(metadata)`
// plus `value(entry)` and get meta lines and a card link from the entry.
const metricConfigs = [
  { label: "Angebote", value: (m) => m.total.toLocaleString("de-DE") },
  { label: "Apotheken", value: (m) => m.pharmacy_count.toLocaleString("de-DE") },
  { label: "Sorten", value: (m) => m.strain_count.toLocaleString("de-DE") },
  {
    label: "Günstigster €/g",
    entry: (m) => m.cheapest_gram,
    value: (entry) => formatEuro(entry.price, "€/g")
  },
  {
    label: "Günstigster €/g THC",
    entry: (m) => m.cheapest_thc_gram,
    value: (entry) => formatEuro(entry.price, "€/g THC")
  },
  {
    label: "Günstigster €/g CBD",
    entry: (m) => m.cheapest_cbd_gram,
    value: (entry) => formatEuro(entry.price, "€/g CBD")
  },
  {
    label: "Höchster THC",
    entry: (m) => m.highest_thc,
    value: (entry) => entry.thc || ""
  },
  {
    label: "Höchster CBD",
    entry: (m) => m.highest_cbd,
    value: (entry) => entry.cbd || ""
  },
  {
    label: "Höchster THC & CBD",
    entry: (m) => m.highest_thc_cbd,
    value: (entry) => [entry.thc, entry.cbd].filter(Boolean).join(" · ")
  }
];

function cardLink(entry) {
  const link = document.createElement("a");
  link.className = "card-link";
  link.href = entry.produkt_url;
  link.target = "_blank";
  link.rel = "noopener";
  link.setAttribute("aria-label", `${entry.name || "Sorte"} bei greenmedical öffnen`);
  return link;
}

// With metadata=null this renders empty skeleton cards (labels only), so the
// header keeps its layout before the data fetch resolves.
function renderMetrics(metadata) {
  elements.metrics.replaceChildren();

  for (const config of metricConfigs) {
    const card = document.createElement("div");
    card.className = "metric";

    const label = document.createElement("div");
    label.className = "metric-label";
    label.textContent = config.label;
    card.appendChild(label);

    const value = document.createElement("div");
    value.className = "metric-value";
    card.appendChild(value);

    if (metadata && config.entry) {
      const entry = config.entry(metadata);
      value.textContent = entry ? config.value(entry) : "";

      const meta = document.createElement("div");
      meta.className = "metric-meta";
      fillMeta(meta, entry);
      card.appendChild(meta);

      if (entry && entry.produkt_url) {
        card.classList.add("linked");
        card.appendChild(cardLink(entry));
      }
    } else if (metadata) {
      value.textContent = config.value(metadata);
    }

    elements.metrics.appendChild(card);
  }
}

function updateGeneratedAt(metadata) {
  const generatedAt = new Date(metadata.generated_at);
  if (!Number.isNaN(generatedAt.valueOf())) {
    elements.updatedAt.textContent = generatedAt.toLocaleString("de-DE", {
      dateStyle: "medium",
      timeStyle: "short"
    });
  }
}

async function loadData() {
  const [rowsResponse, metadataResponse] = await Promise.all([
    fetch("data/flowers.json"),
    fetch("data/metadata.json")
  ]);

  if (!rowsResponse.ok || !metadataResponse.ok) {
    throw new Error("Daten konnten nicht geladen werden.");
  }

  state.rows = await rowsResponse.json();
  const metadata = await metadataResponse.json();
  renderMetrics(metadata);
  updateGeneratedAt(metadata);
  initFilters();
  applyFilters();
}

elements.search.addEventListener("input", applyFilters);
elements.toggleFilters.addEventListener("click", () => {
  const open = elements.filters.hidden;
  elements.filters.hidden = !open;
  elements.toggleFilters.setAttribute("aria-expanded", String(open));
  elements.toggleFilters.classList.toggle("open", open);
});
elements.clear.addEventListener("click", () => {
  elements.search.value = "";
  resetFilters();
  elements.search.focus();
  applyFilters();
});

updateHeader();
renderMetrics(null);
loadData().catch((error) => {
  elements.body.replaceChildren();
  const row = document.createElement("tr");
  const cell = document.createElement("td");
  cell.className = "empty";
  cell.colSpan = columns.length;
  cell.textContent = error.message;
  row.appendChild(cell);
  elements.body.appendChild(row);
});
