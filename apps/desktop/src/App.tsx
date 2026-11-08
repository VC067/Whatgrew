import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { hierarchy, treemap, treemapSquarify } from 'd3-hierarchy';
import {
  Activity, ArrowDownRight, ArrowRight, ArrowUpRight, Check, ChevronLeft, ChevronRight, Clock3, Copy, ExternalLink, Folder, FolderOpen,
  HardDrive, Info, LayoutDashboard, LoaderCircle, Play, RefreshCw, Settings2,
  ShieldCheck, SlidersHorizontal, Sparkles, X, ChartNoAxesColumnIncreasing
} from 'lucide-react';
import type { BigFile, Diff, ExploreChild, Hotspot, Page, ScanDone, ScanProgress, Settings, Snapshot } from './types';

const defaultSettings: Settings = { scanRoot: '', theme: 'system', retention: 30, exclusions: [], schedule: false };
function bytes(value: number, precision = 1): string {
  if (!Number.isFinite(value)) return '—';
  const n = Math.abs(value);
  const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
  let index = 0; let scaled = n;
  while (scaled >= 1024 && index < units.length - 1) { index += 1; scaled /= 1024; }
  return `${scaled.toFixed(index ? precision : 0)} ${units[index]}`;
}
function delta(value: number): string { return `${value > 0 ? '+' : value < 0 ? '−' : ''}${bytes(value)}`; }
function date(value: number): string {
  return new Date(value * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
}
function timeAgo(value: number): string {
  const hours = Math.max(0, (Date.now() - value * 1000) / 3.6e6);
  if (hours < 1) return 'Less than an hour ago';
  if (hours < 24) return `${Math.floor(hours)}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}
function tone(value: number): string { return value > 0 ? 'increase' : value < 0 ? 'decrease' : 'unchanged'; }

function Logo() {
  return <div className="brand-icon" aria-hidden="true"><svg viewBox="0 0 34 34"><circle cx="17" cy="17" r="12.5" fill="none" stroke="currentColor" strokeWidth="2.6"/><path d="M10 20l5-6 5 3 5-7" fill="none" stroke="currentColor" strokeWidth="2.8" strokeLinecap="round" strokeLinejoin="round"/><circle cx="25" cy="10" r="2" fill="currentColor"/></svg></div>;
}

function Header({ page, isScanning, onScan, latest }: { page: Page; isScanning: boolean; onScan: () => void; latest?: Snapshot }) {
  const titles: Record<Page, [string, string]> = {
    overview: ['Overview', 'Your disk, explained.'],
    growth: ['Growth activity', 'Find out what changed between scans.'],
    explore: ['Explore storage', 'Discover where your space is going.'],
    settings: ['Settings', 'Make whatgrew work your way.'],
  };
  return <header className="page-header">
    <div><div className="eyebrow">YOUR WORKSPACE <span className="crumb-slash">/</span> {titles[page][0].toUpperCase()}</div>
      <h1>{titles[page][0]}</h1><p className="subtitle">{titles[page][1]}</p></div>
    <div className="header-actions"><div className="privacy-pill"><span className="privacy-dot"/> 100% offline</div>
      <button className="primary-button" onClick={onScan} disabled={isScanning}>{isScanning ? <LoaderCircle className="spin" size={17}/> : <RefreshCw size={17}/>} {isScanning ? 'Scanning…' : 'Scan now'}</button>
      {latest && <span className="header-last">Last: {timeAgo(latest.taken_at_utc)}</span>}
    </div>
  </header>;
}

function Sidebar({ page, setPage, scans }: { page: Page; setPage: (p: Page) => void; scans: number }) {
  const items: { page: Page; title: string; icon: typeof Activity }[] = [
    { page: 'overview', title: 'Overview', icon: LayoutDashboard },
    { page: 'growth', title: 'Growth activity', icon: ChartNoAxesColumnIncreasing },
    { page: 'explore', title: 'Explore storage', icon: FolderOpen },
    { page: 'settings', title: 'Settings', icon: Settings2 },
  ];
  return <aside className="sidebar">
    <div className="brand"><Logo/><div><div className="brand-name">whatgrew<span className="brand-period">.</span></div><div className="brand-tagline">DISK INSIGHTS</div></div></div>
    <div className="nav-section-label">WORKSPACE</div>
    <nav className="nav-items" aria-label="Main navigation">{items.map(({ page: p, title, icon: Icon }) =>
      <button type="button" className={`nav-item ${page === p ? 'active' : ''}`} key={p} onClick={() => setPage(p)}><Icon size={18}/><span>{title}</span>{p === 'growth' && scans > 1 && <span className="nav-count">{scans}</span>}</button>)}</nav>
    <div className="sidebar-bottom">
      <div className="side-info"><div className="side-info-top"><ShieldCheck size={19}/><span>Privacy by design</span></div>
        <p>Your files stay on your device. No accounts, no cloud, no tracking.</p></div>
      <div className="side-footer"><span className="version-dot"/> whatgrew <span className="muted">v0.1.0</span><span className="side-footer-right">OPEN SOURCE</span></div>
    </div>
  </aside>;
}

function EmptyState({ onScan, title, description, icon: Icon = HardDrive }: { onScan: () => void; title: string; description: string; icon?: typeof HardDrive }) {
  return <div className="empty-state"><div className="empty-orbit"><Icon size={29}/></div><div className="empty-kicker"><Sparkles size={14}/> YOUR FIRST INSIGHT STARTS HERE</div>
    <h2>{title}</h2><p>{description}</p><button className="primary-button" onClick={onScan}><Play size={16} fill="currentColor"/> Take your first snapshot <ArrowRight size={16}/></button>
    <div className="empty-assurance"><ShieldCheck size={16}/> Read-only scanning. Nothing gets deleted or uploaded.</div>
  </div>;
}

function Trend({ snapshots }: { snapshots: Snapshot[] }) {
  const series = [...snapshots].reverse().filter((s) => s.scan_root === snapshots[0]?.scan_root).slice(-16);
  if (series.length === 0) return null;
  const w = 730, h = 116, top = 17, bottom = 15;
  const values = series.map(s => s.volume_free);
  const min = Math.min(...values), max = Math.max(...values);
  const toX = (i: number) => series.length === 1 ? w / 2 : i * (w / (series.length - 1));
  const toY = (v: number) => max === min ? h / 2 : h - bottom - (v - min) / (max - min) * (h - top - bottom);
  const line = series.map((s, i) => `${i ? 'L' : 'M'}${toX(i)},${toY(s.volume_free)}`).join(' ');
  const fill = `${line} L${toX(series.length - 1)},${h} L${toX(0)},${h} Z`;
  return <div className="trend-chart"><svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" role="img" aria-label="History of free disk space">
    <defs><linearGradient id="trendGradient" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor="#756bea" stopOpacity=".19"/><stop offset="1" stopColor="#756bea" stopOpacity="0"/></linearGradient></defs>
    {[30, 64, 98].map(y => <line key={y} x1="0" y1={y} x2={w} y2={y} stroke="var(--line)" strokeDasharray="4 7"/>) }
    <path d={fill} fill="url(#trendGradient)"/><path d={line} fill="none" stroke="#8173ee" strokeWidth="3.2" strokeLinejoin="round" vectorEffect="non-scaling-stroke"/>
    {series.map((s, i) => <circle key={s.file} cx={toX(i)} cy={toY(s.volume_free)} r={i === series.length - 1 ? 5 : 3} stroke="var(--surface)" strokeWidth="2" fill="#756bea"><title>{`${date(s.taken_at_utc)}: ${bytes(s.volume_free)} free`}</title></circle>)}
  </svg><div className="trend-legend"><span>{date(series[0].taken_at_utc)}</span><span>{series.length === 1 ? 'Your baseline' : `${series.length} snapshots`}</span><span>{date(series[series.length - 1].taken_at_utc)}</span></div></div>;
}

function MetricCard({ label, value, detail, icon: Icon, accent, deltaTone }: {
  label: string; value: string; detail: string; icon: typeof HardDrive; accent?: boolean; deltaTone?: string;
}) {
  return <div className={`metric-card ${accent ? 'metric-accent' : ''}`}><div className="metric-label"><span>{label}</span><div className="metric-icon"><Icon size={18}/></div></div>
    <div className={`metric-value ${deltaTone || ''}`}>{value}</div><div className="metric-detail">{detail}</div></div>;
}

function GrowthRow({ hotspot, max, index, onReveal, onCopy }: { hotspot: Hotspot; max: number; index: number; onReveal: (path: string) => void; onCopy: (path: string) => void }) {
  const width = max ? Math.max(2, Math.round(Math.abs(hotspot.delta_bytes) / max * 100)) : 0;
  return <div className="growth-row">
    <div className="growth-row-icon"><Folder size={19}/></div>
    <div className="growth-row-body"><div className="growth-row-head"><span className="growth-name">{hotspot.label || hotspot.name || 'Scan root'}</span>{hotspot.label && <span className="label-badge">Identified</span>}</div>
      <div className="growth-path" title={hotspot.path}>{hotspot.path}</div><div className="growth-meter"><div className={`growth-meter-fill ${tone(hotspot.delta_bytes)}`} style={{width: `${width}%`}} /></div>
      {hotspot.hint && <div className="row-hint"><Info size={12}/>{hotspot.hint}</div>}
    </div>
    <div className="growth-row-end"><div className={`growth-amount ${tone(hotspot.delta_bytes)}`}>{delta(hotspot.delta_bytes)}</div><div className="growth-share">#{String(index + 1).padStart(2, '0')} · {bytes(hotspot.new_bytes)} now</div>
      <div className="row-actions"><button aria-label="Copy path" title="Copy path" onClick={() => onCopy(hotspot.path)}><Copy size={14}/></button><button aria-label="Reveal in file manager" title="Reveal in Finder / Explorer" onClick={() => onReveal(hotspot.path)}><ExternalLink size={14}/></button></div>
    </div>
  </div>;
}

function Overview({ snapshots, diff, onScan, setPage, onReveal, onCopy }: {
  snapshots: Snapshot[]; diff: Diff | null; onScan: () => void; setPage: (p: Page) => void;
  onReveal: (s: string) => void; onCopy: (s: string) => void;
}) {
  const latest = snapshots[0];
  if (!latest) return <EmptyState onScan={onScan} title="See where your space goes." description="whatgrew remembers how your disk changes over time. Scan once to set a baseline; scan again to see exactly what's growing."/>;
  const isNew = !diff;
  const positive = diff?.hotspots.filter(h => h.delta_bytes > 0) ?? [];
  const max = Math.max(1, ...positive.map(h => h.delta_bytes));
  return <div className="content-stack">
    {isNew && <div className="baseline-banner"><div className="baseline-icon"><Check size={20}/></div><div><strong>Your baseline is ready</strong><span>Run another scan later to see exactly which folders have grown.</span></div><button onClick={onScan}>Scan again <ArrowRight size={16}/></button></div>}
    <div className="metric-grid">
      <MetricCard label="Space used in scan" value={bytes(latest.alloc_bytes)} detail="Files inside your scan root" icon={HardDrive}/>
      <MetricCard label="Change since last scan" value={diff ? delta(diff.growth_bytes) : '—'} detail={diff ? `Since ${date(diff.from_utc)}` : 'Waiting for a second scan'} icon={diff && diff.growth_bytes < 0 ? ArrowDownRight : ArrowUpRight} deltaTone={diff ? tone(diff.growth_bytes) : ''} accent/>
      <MetricCard label="Available on volume" value={bytes(latest.volume_free)} detail={diff ? `${delta(diff.free_delta_bytes)} since previous scan` : 'At the time of your scan'} icon={Activity}/>
    </div>
    <div className="dashboard-grid">
      <section className="panel trend-panel"><div className="panel-title"><div><h2>Free space over time</h2><p>See how your available storage changes</p></div><div className="small-icon"><Activity size={18}/></div></div><Trend snapshots={snapshots}/></section>
      <section className="panel tracking-panel"><div className="panel-title"><div><h2>Tracking status</h2><p>Your latest disk snapshot</p></div><div className="status-pill"><span/> Active</div></div>
        <div className="tracking-detail"><span>Scanned directory</span><strong title={latest.scan_root}>{latest.scan_root}</strong></div>
        <div className="tracking-detail"><span>Last scan</span><strong>{date(latest.taken_at_utc)}</strong></div>
        <div className="tracking-detail"><span>Snapshots saved</span><strong>{snapshots.length}</strong></div>
        {latest.unreadable_dirs > 0 && <div className="warning-small"><Info size={15}/>{latest.unreadable_dirs} paths couldn't be read; totals may be incomplete.</div>}
      </section>
    </div>
    <section className="panel growers-panel"><div className="panel-title"><div><h2>Top growers</h2><p>{diff ? 'Folders that used more space since the last scan' : 'Available after your second snapshot'}</p></div><button className="text-action" onClick={() => setPage(diff ? 'growth' : 'explore')}>{diff ? 'All activity' : 'Explore files'} <ArrowRight size={16}/></button></div>
      {positive.length ? positive.slice(0, 4).map((h, index) => <GrowthRow key={h.path} hotspot={h} max={max} index={index} onReveal={onReveal} onCopy={onCopy}/>) :
        <div className="quiet-empty"><div className="quiet-empty-icon"><ChartNoAxesColumnIncreasing size={24}/></div><strong>{isNew ? 'Not enough history yet' : 'No significant growth detected'}</strong><p>{isNew ? 'The next scan will reveal what changed. In the meantime, explore your biggest folders.' : 'Nothing crossed the current growth threshold between these two scans.'}</p></div>}
    </section>
    <div className="footnote"><ShieldCheck size={16}/> Read-only, private, and completely offline. whatgrew will never delete your files.</div>
  </div>;
}

function Growth({ snapshots, diff, selectedOld, setSelectedOld, onReveal, onCopy, onScan }: {
  snapshots: Snapshot[]; diff: Diff | null; selectedOld: string; setSelectedOld: (s: string) => void;
  onReveal: (s: string) => void; onCopy: (s: string) => void; onScan: () => void;
}) {
  const latest = snapshots[0];
  const older = snapshots.filter(s => s.file !== latest?.file && s.scan_root === latest?.scan_root);
  if (older.length === 0) return <div className="panel large-empty"><div className="empty-orbit small"><Clock3 size={25}/></div><h2>Every change has a starting point.</h2><p>Run a second scan after some time has passed. Your first snapshot is already saved, ready for comparison.</p><button className="primary-button" onClick={onScan}><RefreshCw size={16}/> Scan now</button></div>;
  const growing = diff?.hotspots.filter(h => h.delta_bytes > 0) ?? [];
  const shrinking = diff?.hotspots.filter(h => h.delta_bytes < 0) ?? [];
  const max = Math.max(1, ...growing.map(h => h.delta_bytes));
  return <div className="content-stack"><div className="compare-toolbar panel"><div className="compare-left"><div className="compare-icon"><SlidersHorizontal size={20}/></div><div><strong>Compare snapshots</strong><span>Choose a starting point to see what changed</span></div></div><div className="compare-right">
    <select aria-label="Previous snapshot" value={selectedOld || older[0]?.file || ''} onChange={e => setSelectedOld(e.target.value)}>{older.map(s => <option key={s.file} value={s.file}>{date(s.taken_at_utc)}</option>)}</select><ArrowRight size={18}/><span className="latest-chip">{date(latest.taken_at_utc)}</span>
  </div></div>
  <div className="growth-summary"><div><span>NET DISK GROWTH</span><strong className={tone(diff?.growth_bytes ?? 0)}>{diff ? delta(diff.growth_bytes) : '—'}</strong><small>Change in allocated file sizes within scanned root</small></div><div className="summary-divider"/><div><span>FREE SPACE CHANGE</span><strong className={tone(-(diff?.free_delta_bytes ?? 0))}>{diff ? delta(diff.free_delta_bytes) : '—'}</strong><small>Physical free space reported by volume</small></div><div className="summary-divider"/><div><span>CHANGED FOLDERS</span><strong>{diff?.hotspots.length ?? '—'}</strong><small>Attributed growth and shrinkage hotspots</small></div></div>
  <section className="panel growers-panel"><div className="panel-title"><div><h2>What took up space?</h2><p>Ranked by the biggest increase in allocated bytes</p></div><span className="result-count">{growing.length} growing</span></div>
    {growing.length > 0 ? growing.map((h, i) => <GrowthRow key={h.path} hotspot={h} max={max} index={i} onReveal={onReveal} onCopy={onCopy}/>) : <div className="quiet-empty"><strong>No growing folders found</strong><p>There may be no meaningful growth between these scans.</p></div>}
  </section>
  {shrinking.length > 0 && <section className="panel growers-panel"><div className="panel-title"><div><h2>Space you got back</h2><p>Folders that got smaller</p></div><span className="result-count green">{shrinking.length} shrinking</span></div>{shrinking.map((h, i) => <GrowthRow key={h.path} hotspot={h} max={Math.max(1, ...shrinking.map(s => Math.abs(s.delta_bytes)))} index={i} onReveal={onReveal} onCopy={onCopy}/>)}</section>}
  </div>;
}

function Treemap({ children, colorGrowth, onDrill }: { children: ExploreChild[]; colorGrowth: boolean; onDrill: (p: string) => void }) {
  const leaves = useMemo(() => {
    type NodeDatum = { name: string; value?: number; bytes?: number; path?: string; delta?: number | null; children?: NodeDatum[] };
    const input: NodeDatum = { name: 'root', children: children.filter(c => c.bytes > 0).map(c => ({ ...c, value: c.bytes })) };
    const root = hierarchy<NodeDatum>(input).sum(d => d.value ?? 0).sort((a, b) => (b.value ?? 0) - (a.value ?? 0));
    treemap<NodeDatum>().size([944, 436]).paddingOuter(4).paddingInner(4).tile(treemapSquarify.ratio(1.35))(root);
    return root.leaves();
  }, [children]);
  const palette = ['#8077e9', '#5d9ee9', '#47b8b3', '#e6ad73', '#9a8fe3', '#6ea79c', '#d08dad', '#839ab3', '#b9aacd'];
  if (!leaves.length) return <div className="quiet-empty">This folder has no tracked subfolders or files.</div>;
  return <svg viewBox="0 0 944 436" role="img" aria-label="Treemap of allocated disk space" className="treemap-svg" preserveAspectRatio="none">
    {leaves.map((leaf, i) => {
      const x = leaf.x0, y = leaf.y0, w = leaf.x1 - leaf.x0, h = leaf.y1 - leaf.y0;
      const item = leaf.data as unknown as ExploreChild;
      const interactive = !!item.path;
      const bg = colorGrowth ? (item.delta !== null && item.delta > 0 ? '#e38370' : item.delta !== null && item.delta < 0 ? '#52b79a' : '#8791a5') : palette[i % palette.length];
      return <g key={item.path || `other-${i}`} onClick={() => interactive && onDrill(item.path)} className={interactive ? 'treemap-clickable' : ''} tabIndex={interactive ? 0 : undefined} role={interactive ? 'button' : undefined}
        onKeyDown={e => { if (interactive && (e.key === 'Enter' || e.key === ' ')) { e.preventDefault(); onDrill(item.path); } }} aria-label={`${item.name}, ${bytes(item.bytes)}`}>
        <rect x={x} y={y} width={Math.max(0, w)} height={Math.max(0, h)} fill={bg} rx="9" />
        {w > 95 && h > 42 && <><text x={x + 14} y={y + 27} fill="#fff" fontWeight="650" fontSize={Math.min(15, Math.max(10, w / 9))}>{item.name.length > Math.floor(w / 10) ? `${item.name.slice(0, Math.max(1, Math.floor(w / 10) - 2))}…` : item.name}</text>{h > 64 && <text x={x + 14} y={y + 48} fill="#ffffffcc" fontSize="13">{bytes(item.bytes)}</text>}</>}
        <title>{`${item.name}: ${bytes(item.bytes)}${item.delta !== null ? ` (${delta(item.delta)})` : ''}`}</title>
      </g>;
    })}
  </svg>;
}

function Explore({ latest, old, onReveal, onCopy }: { latest?: Snapshot; old?: Snapshot; onReveal: (s: string) => void; onCopy: (s: string) => void }) {
  const [path, setPath] = useState('');
  const [children, setChildren] = useState<ExploreChild[]>([]);
  const [bigFiles, setBigFiles] = useState<BigFile[]>([]);
  const [colorGrowth, setColorGrowth] = useState(false);
  const [loading, setLoading] = useState(false);
  useEffect(() => { setPath(''); }, [latest?.file]);
  useEffect(() => {
    if (!latest) return;
    let active = true;
    setLoading(true);
    invoke<ExploreChild[]>('get_children', { newFile: latest.file, path, oldFile: old?.file ?? null })
      .then(result => { if (active) setChildren(result); })
      .catch(() => { if (active) setChildren([]); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [latest, old, path]);
  useEffect(() => {
    if (!latest) return;
    invoke<BigFile[]>('get_big_files', { file: latest.file }).then(setBigFiles).catch(() => setBigFiles([]));
  }, [latest]);
  if (!latest) return <div className="quiet-empty">Scan a directory to explore its space usage.</div>;
  const parts = path ? path.split('/') : [];
  return <div className="content-stack">
    <section className="panel treemap-panel"><div className="panel-title"><div><h2>Storage map</h2><p>Click a block to explore that folder</p></div><div className="treemap-tools"><label className="switch-label"><input type="checkbox" checked={colorGrowth} onChange={e => setColorGrowth(e.target.checked)} disabled={!old}/><span className="switch"/> Color by growth</label></div></div>
      <div className="breadcrumbs"><button onClick={() => setPath('')}><HardDrive size={15}/>{latest.scan_root.split(/[\\/]/).filter(Boolean).pop() || latest.scan_root}</button>{parts.map((p, i) => <span key={i} className="crumb-part"><ChevronRight size={14}/><button onClick={() => setPath(parts.slice(0, i + 1).join('/'))}>{p}</button></span>)}</div>
      <div className="treemap-wrap">{loading ? <div className="treemap-loading"><LoaderCircle className="spin" size={26}/> Loading map…</div> : <Treemap children={children} colorGrowth={colorGrowth} onDrill={setPath}/>}</div>
      <div className="map-key">{colorGrowth && old ? <><span><i className="legend-dot red"/> Grew</span><span><i className="legend-dot green"/> Shrunk</span><span><i className="legend-dot gray"/> Unchanged / unknown</span></> : <><span><i className="legend-dot purple"/> Sized by disk allocation</span><span>Smaller items include files and pruned folders</span></>}</div>
    </section>
    <div className="explorer-grid"><section className="panel explorer-list"><div className="panel-title"><div><h2>Inside this folder</h2><p>Largest items first</p></div><span className="result-count">{children.length} groups</span></div>
      {children.slice(0, 12).map((child, i) => <button className="folder-line" disabled={!child.path} key={`${child.path}-${i}`} onClick={() => setPath(child.path)}><Folder size={17}/><span className="folder-line-name">{child.name}</span><strong>{bytes(child.bytes)}</strong>{child.path ? <ChevronRight size={16}/> : <span className="folder-placeholder"/>}</button>)}
      {!children.length && <div className="quiet-empty">No tracked folders here.</div>}
      {path && <button className="subtle-link" onClick={() => setPath(parts.slice(0, -1).join('/'))}><ChevronLeft size={15}/> Back one level</button>}
      </section><section className="panel explorer-list"><div className="panel-title"><div><h2>Large files</h2><p>Top individual files from this scan</p></div><span className="result-count">{bigFiles.length} files</span></div>
      {bigFiles.slice(0, 8).map(file => <div className="file-line" key={file.path}><div className="file-icon"><HardDrive size={16}/></div><div title={file.path} className="file-line-name">{file.name}<small>{file.path}</small></div><strong>{bytes(file.bytes)}</strong><div className="file-tools"><button onClick={() => onCopy(file.path)} title="Copy path"><Copy size={13}/></button><button onClick={() => onReveal(file.path)} title="Reveal"><ExternalLink size={13}/></button></div></div>)}
      {!bigFiles.length && <div className="quiet-empty">No files above the 50 MB tracking threshold.</div>}
    </section></div>
  </div>;
}

function SettingsPage({ settings, setSettings, onSave, onSchedule, saving }: {
  settings: Settings; setSettings: (settings: Settings) => void; onSave: () => void; onSchedule: (enabled: boolean) => void; saving: boolean;
}) {
  const [patterns, setPatterns] = useState(settings.exclusions.join('\n'));
  const edit = (key: keyof Settings, value: unknown) => setSettings({ ...settings, [key]: value });
  return <div className="settings-stack">
    <section className="panel settings-section"><div className="settings-section-icon"><FolderOpen size={21}/></div><div className="settings-body"><h2>Scan preferences</h2><p>Choose the directory to track. Everything inside is read-only.</p>
      <label className="field-label" htmlFor="scan-root">Folder to scan</label><input id="scan-root" className="text-input" value={settings.scanRoot} onChange={e => edit('scanRoot', e.target.value)} placeholder="/Users/you or C:\\"/><small className="field-hint">The default is your home folder on macOS and the system drive on Windows.</small>
      <label className="field-label" htmlFor="exclusions">Exclude paths (optional)</label><textarea id="exclusions" rows={3} value={patterns} onChange={e => { setPatterns(e.target.value); edit('exclusions', e.target.value.split('\n').map(s => s.trim()).filter(Boolean)); }} placeholder={'**/node_modules\n**/.git/**'} /><small className="field-hint">One glob pattern per line. Relative to your scan root.</small>
    </div></section>
    <section className="panel settings-section"><div className="settings-section-icon"><Clock3 size={21}/></div><div className="settings-body"><h2>History & automation</h2><p>Keep a rolling record of the space your files consume.</p>
      <div className="setting-line"><div><strong>Weekly automatic scan</strong><span>Every Sunday at 10 AM via your operating system scheduler</span></div><label className="switch-label" aria-label="Weekly schedule"><input type="checkbox" checked={settings.schedule} onChange={e => onSchedule(e.target.checked)}/><span className="switch"/></label></div>
      <div className="setting-line"><div><strong>Snapshots to keep</strong><span>Old snapshots are automatically pruned after a scan</span></div><select className="short-select" value={settings.retention} onChange={e => edit('retention', Number(e.target.value))}><option value={10}>10</option><option value={30}>30</option><option value={60}>60</option><option value={100}>100</option></select></div>
    </div></section>
    <section className="panel settings-section"><div className="settings-section-icon"><SlidersHorizontal size={21}/></div><div className="settings-body"><h2>Appearance</h2><p>Keep things comfortable, day or night.</p><div className="setting-line"><div><strong>Theme</strong><span>Choose your preferred appearance</span></div><select className="short-select" value={settings.theme} onChange={e => edit('theme', e.target.value)}><option value="system">System</option><option value="light">Light</option><option value="dark">Dark</option></select></div></div></section>
    <div className="settings-actions"><span><ShieldCheck size={15}/> All data stays on this computer.</span><button className="primary-button" onClick={onSave} disabled={saving}>{saving ? <LoaderCircle className="spin" size={17}/> : <Check size={17}/>} Save changes</button></div>
  </div>;
}

export default function App() {
  const [page, setPage] = useState<Page>('overview');
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [selectedOld, setSelectedOld] = useState('');
  const [comparison, setComparison] = useState<Diff | null>(null);
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [saving, setSaving] = useState(false);
  const [toast, setToast] = useState('');

  const latest = snapshots[0];
  const previous = snapshots.find(s => s.file === selectedOld) || snapshots.find(s => latest && s.file !== latest.file && s.scan_root === latest.scan_root);
  const displayToast = useCallback((msg: string) => { setToast(msg); }, []);
  const refresh = useCallback(async () => {
    try {
      const list = await invoke<Snapshot[]>('list_snapshots');
      setSnapshots(list);
      setSelectedOld(old => list.some(s => s.file === old && s.file !== list[0]?.file) ? old : (list.find(s => s.file !== list[0]?.file && s.scan_root === list[0]?.scan_root)?.file || ''));
    } catch (error) { displayToast(String(error)); }
  }, [displayToast]);

  useEffect(() => {
    void invoke<Settings>('get_settings').then(setSettings).catch(e => displayToast(String(e)));
    void refresh();
  }, [refresh, displayToast]);
  useEffect(() => {
    if (!latest || !previous) { setComparison(null); return; }
    let active = true;
    void invoke<Diff>('get_diff', { a: previous.file, b: latest.file, minMb: 10 }).then(d => { if (active) setComparison(d); })
      .catch(e => { if (active) { setComparison(null); displayToast(String(e)); } });
    return () => { active = false; };
  }, [latest, previous, displayToast]);
  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const apply = () => document.documentElement.dataset.theme = settings.theme === 'system' ? (mq.matches ? 'dark' : 'light') : settings.theme;
    apply(); mq.addEventListener('change', apply);
    return () => mq.removeEventListener('change', apply);
  }, [settings.theme]);
  useEffect(() => {
    const listeners = Promise.all([
      listen<ScanProgress>('scan-progress', e => setProgress(e.payload)),
      listen<ScanDone>('scan-done', e => {
        setScanning(false); setProgress(null);
        if (e.payload.error) displayToast(e.payload.error);
        else { displayToast(e.payload.unreadableDirs > 0 ? `Snapshot saved with ${e.payload.unreadableDirs} unreadable paths.` : 'Snapshot saved successfully.'); void refresh(); }
      }),
    ]);
    return () => { void listeners.then(handles => handles.forEach(dispose => dispose())); };
  }, [displayToast, refresh]);
  useEffect(() => { if (!toast) return; const timer = window.setTimeout(() => setToast(''), 4300); return () => clearTimeout(timer); }, [toast]);

  const startScan = async () => {
    if (scanning) return;
    if (!settings.scanRoot.trim()) { displayToast('Choose a folder to scan in Settings.'); setPage('settings'); return; }
    try { await invoke('start_scan', { root: settings.scanRoot }); setScanning(true); setProgress({ files: 0, bytes: 0, directories: 0, current_dir: settings.scanRoot }); }
    catch (err) { displayToast(String(err)); }
  };
  const saveSettings = async () => {
    setSaving(true);
    try { await invoke('set_settings', { settings }); displayToast('Preferences saved.'); }
    catch (e) { displayToast(String(e)); }
    finally { setSaving(false); }
  };
  const toggleSchedule = async (enabled: boolean) => {
    try { await invoke('set_schedule', { enabled, root: settings.scanRoot }); setSettings(s => ({ ...s, schedule: enabled })); displayToast(enabled ? 'Weekly scans enabled.' : 'Weekly scans disabled.'); }
    catch (e) { displayToast(`Could not change schedule: ${String(e)}`); }
  };
  const reveal = async (path: string) => { try { await invoke('reveal', { path }); } catch (e) { displayToast(String(e)); } };
  const copy = async (path: string) => { try { await navigator.clipboard.writeText(path); displayToast('Path copied.'); } catch { displayToast('Clipboard unavailable.'); } };
  const pageBody = useMemo(() => {
    if (page === 'overview') return <Overview snapshots={snapshots} diff={comparison} onScan={startScan} setPage={setPage} onReveal={reveal} onCopy={copy}/>;
    if (page === 'growth') return <Growth snapshots={snapshots} diff={comparison} selectedOld={previous?.file || ''} setSelectedOld={setSelectedOld} onScan={startScan} onReveal={reveal} onCopy={copy}/>;
    if (page === 'explore') return <Explore latest={latest} old={previous} onReveal={reveal} onCopy={copy}/>;
    return <SettingsPage settings={settings} setSettings={setSettings} onSave={saveSettings} onSchedule={toggleSchedule} saving={saving}/>;
  // Event handlers are regenerated every render; this memo is an organizational convenience.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [page, snapshots, comparison, previous, latest, settings, scanning, saving]);

  return <div className="app-shell"><Sidebar page={page} setPage={setPage} scans={snapshots.length}/><main className="main"><div className="main-inner"><Header page={page} isScanning={scanning} onScan={startScan} latest={latest}/>{pageBody}<footer className="footer"><span>© whatgrew</span><span>Built to understand, not to clean.</span><span><ShieldCheck size={14}/> LOCAL ONLY</span></footer></div></main>
    {scanning && <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label="Scanning disk"><div className="scan-modal"><div className="scan-animated"><HardDrive size={30}/><div className="orbit-one"/><div className="orbit-two"/></div><h2>Finding what's taking space</h2><p>Reading file metadata without changing any files.</p><div className="scan-counters"><div><strong>{(progress?.files || 0).toLocaleString()}</strong><span>FILES SEEN</span></div><div><strong>{(progress?.directories || 0).toLocaleString()}</strong><span>FOLDERS</span></div><div><strong>{bytes(progress?.bytes || 0)}</strong><span>ALLOCATED</span></div></div><div className="scan-progress-track"><div className="scan-indeterminate"/></div><div className="scan-current" title={progress?.current_dir}>Scanning {progress?.current_dir || '…'}</div><button className="cancel-button" onClick={() => { void invoke('cancel_scan'); }}><X size={17}/> Cancel scan</button></div></div>}
    {toast && <div className="toast" role="status"><Info size={17}/>{toast}<button aria-label="Dismiss" onClick={() => setToast('')}><X size={16}/></button></div>}
  </div>;
}
