//! Inlined CSS for the vajra HTML report.
//!
//! Extracted from the design system in `effect-report/report.py`.

/// Returns the full CSS stylesheet as a string constant.
pub fn report_css() -> &'static str {
    r##"
@page {
    size: A4;
    margin: 20mm 18mm 25mm 18mm;
    @top-right {
        content: "CONFIDENTIAL";
        font-size: 7pt;
        color: #94a3b8;
        font-family: 'Inter', system-ui, sans-serif;
    }
    @bottom-center {
        content: counter(page) " / " counter(pages);
        font-size: 8pt;
        color: #64748b;
        font-family: 'Inter', system-ui, sans-serif;
    }
    @bottom-left {
        content: "vajra deterministic analysis";
        font-size: 7pt;
        color: #94a3b8;
        font-family: 'Inter', system-ui, sans-serif;
    }
}

@page :first {
    margin-top: 0;
    @top-right { content: none; }
    @bottom-center { content: none; }
    @bottom-left { content: none; }
}

:root {
    --purple: #8b5cf6;
    --blue: #3b82f6;
    --amber: #f59e0b;
    --red: #ef4444;
    --green: #22c55e;
    --slate-50: #f8fafc;
    --slate-100: #f1f5f9;
    --slate-200: #e2e8f0;
    --slate-300: #cbd5e1;
    --slate-400: #94a3b8;
    --slate-500: #64748b;
    --slate-600: #475569;
    --slate-700: #334155;
    --slate-800: #1e293b;
    --slate-900: #0f172a;
}

* { margin: 0; padding: 0; box-sizing: border-box; }

body {
    font-family: 'Inter', system-ui, -apple-system, sans-serif;
    font-size: 9pt;
    line-height: 1.5;
    color: var(--slate-800);
    background: white;
}

/* === COVER PAGE === */
.cover {
    page-break-after: always;
    height: 100vh;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    text-align: center;
    background: linear-gradient(135deg, #0f172a 0%, #1e1b4b 40%, #312e81 100%);
    color: white;
    margin: -20mm -18mm 0 -18mm;
    padding: 40mm 30mm;
}

.cover h1 {
    font-size: 36pt;
    font-weight: 800;
    letter-spacing: -0.03em;
    background: linear-gradient(135deg, #c084fc, #60a5fa);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    margin-bottom: 12px;
}
.cover h2 {
    font-size: 14pt;
    font-weight: 400;
    color: #94a3b8;
    margin-bottom: 40px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
}
.cover-meta {
    font-size: 9pt;
    color: #64748b;
    line-height: 2;
}
.cover-meta strong { color: #94a3b8; }
.cover-divider {
    width: 120px;
    height: 2px;
    background: linear-gradient(90deg, #8b5cf6, #3b82f6);
    margin: 30px auto;
    border-radius: 1px;
}
.cover-badge {
    display: inline-block;
    padding: 4px 14px;
    border: 1px solid #4c1d95;
    border-radius: 20px;
    font-size: 7.5pt;
    color: #a78bfa;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    margin-top: 20px;
}

/* === SECTIONS === */
.section { page-break-before: always; }
.section:first-of-type { page-break-before: auto; }
.section-continue { page-break-before: avoid; }

table { page-break-inside: auto; }
tr { page-break-inside: avoid; }
thead { display: table-header-group; }
.callout { page-break-inside: avoid; }
.stat-grid { page-break-inside: avoid; }
.health-item { page-break-inside: avoid; }

h1 {
    font-size: 20pt;
    font-weight: 800;
    color: var(--slate-900);
    letter-spacing: -0.02em;
    margin: 0 0 6px 0;
    padding-bottom: 8px;
    border-bottom: 3px solid;
    border-image: linear-gradient(90deg, var(--purple), var(--blue)) 1;
}

h2 {
    font-size: 13pt;
    font-weight: 700;
    color: var(--slate-800);
    margin: 18px 0 8px 0;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--slate-200);
}

h3 {
    font-size: 10pt;
    font-weight: 700;
    color: var(--slate-700);
    margin: 14px 0 6px 0;
}

p { margin: 6px 0; color: var(--slate-700); }

/* === TABLES === */
table {
    width: 100%;
    border-collapse: collapse;
    margin: 10px 0 16px 0;
    font-size: 8pt;
}

thead th {
    background: var(--slate-800);
    color: white;
    font-weight: 600;
    font-size: 7pt;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 6px 8px;
    text-align: left;
    border: none;
}

tbody td {
    padding: 5px 8px;
    border-bottom: 1px solid var(--slate-100);
    vertical-align: middle;
}

tbody tr:nth-child(even) { background: var(--slate-50); }
tbody tr:hover { background: #ede9fe; }

.num { text-align: right; font-variant-numeric: tabular-nums; }
.mono { font-family: 'JetBrains Mono', 'Fira Code', monospace; font-size: 7.5pt; }
.dim { color: var(--slate-400); }
.label-cell { font-weight: 500; max-width: 200px; }

/* === GRADE COLORS === */
.grade-a { color: var(--green); }
.grade-b { color: var(--blue); }
.grade-c { color: var(--amber); }
.grade-f { color: var(--red); }
.winner { font-weight: 700; }

/* === TAGS === */
.tag {
    display: inline-block;
    padding: 1px 6px;
    border-radius: 3px;
    font-size: 6.5pt;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
}

/* === BARS === */
.bar {
    height: 10px;
    background: linear-gradient(90deg, var(--purple), var(--blue));
    border-radius: 2px;
    min-width: 2px;
}

/* === CALLOUT BOXES === */
.callout {
    padding: 12px 16px;
    border-radius: 6px;
    margin: 12px 0;
    font-size: 8.5pt;
}
.callout-critical {
    background: #fef2f2;
    border-left: 4px solid var(--red);
}
.callout-warning {
    background: #fffbeb;
    border-left: 4px solid var(--amber);
}
.callout-info {
    background: #eff6ff;
    border-left: 4px solid var(--blue);
}
.callout-good {
    background: #f0fdf4;
    border-left: 4px solid var(--green);
}
.callout strong { display: block; margin-bottom: 4px; }

/* === HEALTH CARD === */
.health-card {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    margin: 12px 0;
}
.health-item {
    padding: 10px 14px;
    border-radius: 6px;
    background: var(--slate-50);
    border: 1px solid var(--slate-200);
}
.health-item .metric {
    font-size: 22pt;
    font-weight: 800;
    letter-spacing: -0.03em;
    line-height: 1.1;
}
.health-item .label {
    font-size: 7pt;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--slate-500);
    margin-top: 2px;
}

/* === STAT ROW === */
.stat-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
    margin: 12px 0;
}
.stat-box {
    text-align: center;
    padding: 10px;
    background: var(--slate-50);
    border-radius: 6px;
    border: 1px solid var(--slate-200);
}
.stat-box .val {
    font-size: 18pt;
    font-weight: 800;
    letter-spacing: -0.02em;
}
.stat-box .desc {
    font-size: 6.5pt;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--slate-500);
    margin-top: 2px;
}

/* === COLUMNS === */
.two-col {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
}

/* === CHART-LIKE === */
.hbar-row {
    display: flex;
    align-items: center;
    margin: 3px 0;
    font-size: 7.5pt;
}
.hbar-label {
    width: 120px;
    text-align: right;
    padding-right: 8px;
    font-weight: 500;
    color: var(--slate-600);
    white-space: nowrap;
    overflow: hidden;
}
.hbar-track {
    flex: 1;
    height: 14px;
    background: var(--slate-100);
    border-radius: 3px;
    overflow: hidden;
}
.hbar-fill {
    height: 100%;
    border-radius: 3px;
    display: flex;
    align-items: center;
    padding-left: 6px;
    font-size: 6.5pt;
    font-weight: 600;
    color: white;
}
.hbar-fill.purple { background: linear-gradient(90deg, #8b5cf6, #a78bfa); }
.hbar-fill.blue { background: linear-gradient(90deg, #3b82f6, #60a5fa); }
.hbar-fill.amber { background: linear-gradient(90deg, #f59e0b, #fbbf24); }
.hbar-fill.red { background: linear-gradient(90deg, #ef4444, #f87171); }
.hbar-fill.green { background: linear-gradient(90deg, #22c55e, #4ade80); }
.hbar-fill.slate { background: linear-gradient(90deg, #64748b, #94a3b8); }

.page-break { page-break-before: always; }
.no-orphan { page-break-inside: avoid; }

/* Tighten for print */
@media print {
    body { font-size: 8.5pt; }
}
"##
}
