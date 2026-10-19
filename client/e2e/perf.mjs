// Times the client under load, see perf.sh. For each scenario it drives real mouse input and
// reports the main thread's busy time per step (script, layout, style) and how long frames take.
// PROFILE=<scenario> also lists where the script time went.
import puppeteer from "puppeteer-core";

const base = "http://127.0.0.1:3412";
const browser = await puppeteer.launch({ executablePath: process.env.CHROMIUM ?? "chromium", args: ["--no-sandbox"], headless: true });
const page = await browser.newPage();
await page.setViewport({ width: 1400, height: 900 });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
const cdp = await page.createCDPSession();
await cdp.send("Performance.enable");

await page.goto(`${base}/#/plans`, { waitUntil: "networkidle0" });
await page.type("input[type=email]", "root@localhost");
await page.type("input[type=password]", "password123");
await page.click("button[type=submit]");
await page.waitForSelector(".page", { timeout: 20000 });
await sleep(3000);

const metrics = async () => Object.fromEntries((await cdp.send("Performance.getMetrics")).metrics.map((m) => [m.name, m.value]));
const frames = () => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
await page.evaluate(() => {
  window.__frames = [];
  let last = performance.now();
  const tick = (now) => { window.__frames.push(now - last); last = now; requestAnimationFrame(tick); };
  requestAnimationFrame(tick);
});
const percentile = (xs, p) => xs.length ? [...xs].sort((a, b) => a - b)[Math.min(xs.length - 1, Math.floor(xs.length * p))] : 0;
const rows = [];

async function scenario(name, steps, step) {
  console.error("scenario", name);
  const profiling = process.env.PROFILE === name;
  await frames();
  await page.evaluate(() => { window.__frames = []; });
  if (profiling) { await cdp.send("Profiler.enable"); await cdp.send("Profiler.start"); }
  const before = await metrics();
  for (let i = 0; i < steps; i++) { await step(i); await frames(); }
  const after = await metrics();
  const lengths = await page.evaluate(() => window.__frames);
  const busy = (k) => ((after[k] - before[k]) * 1000) / steps;
  rows.push({
    scenario: name,
    "busy ms/step": +busy("TaskDuration").toFixed(2),
    "script": +busy("ScriptDuration").toFixed(2),
    "layout": +busy("LayoutDuration").toFixed(2),
    "style": +busy("RecalcStyleDuration").toFixed(2),
    "frame p50": +percentile(lengths, 0.5).toFixed(1),
    "frame p95": +percentile(lengths, 0.95).toFixed(1),
    "frame max": +Math.max(0, ...lengths).toFixed(1),
  });
  if (profiling) {
    const { profile } = await cdp.send("Profiler.stop");
    const self = new Map();
    const byId = new Map(profile.nodes.map((n) => [n.id, n]));
    const dt = profile.timeDeltas;
    profile.samples.forEach((id, i) => {
      const f = byId.get(id).callFrame;
      const key = `${f.functionName || "(anonymous)"} ${f.url.split("/").pop()}:${f.lineNumber}`;
      self.set(key, (self.get(key) ?? 0) + (dt[i] ?? 0) / 1000);
    });
    console.log(`\nwhere ${name} spent its script time (ms in total)`);
    [...self.entries()].sort((a, b) => b[1] - a[1]).slice(0, 25).forEach(([k, v]) => console.log(v.toFixed(0).padStart(7), k));
  }
}

const box = (sel) => page.evaluate((sel) => { const r = document.querySelector(sel).getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height }; }, sel);

// the first plan
await page.goto(`${base}/#/plans`, { waitUntil: "networkidle0" });
await sleep(1000);
await page.evaluate(() => { const a = [...document.querySelectorAll("a")].find((a) => a.href.includes("#/plan/")); a.click(); });
await page.waitForSelector("canvas.gantt", { timeout: 20000 });
await sleep(2500);
const g = await box("canvas.gantt");
const wander = (i, w, h, x0, y0) => [x0 + ((i * 37) % (w - 40)) + 20, y0 + 70 + ((i * 53) % (h - 100))];
await page.mouse.move(g.x + 100, g.y + 100);
await scenario("gantt hover", 120, (i) => { const [x, y] = wander(i, g.w, g.h, g.x, g.y); return page.mouse.move(x, y); });
await scenario("gantt wheel scroll", 100, () => page.mouse.wheel({ deltaX: 30, deltaY: 60 }));
await scenario("gantt wheel zoom", 40, (i) => { page.keyboard.down("Shift"); return page.mouse.wheel({ deltaY: i % 2 ? 100 : -100 }).then(() => page.keyboard.up("Shift")); });
await page.mouse.move(g.x + 400, g.y + 300);
await page.mouse.down();
await scenario("gantt drag pan", 120, (i) => page.mouse.move(g.x + 400 + Math.sin(i / 6) * 250, g.y + 300 + Math.cos(i / 6) * 120));
await page.mouse.up();
await page.mouse.move(g.x + 400, g.y + 300);
await page.keyboard.down("Shift");
await scenario("gantt shift hover", 120, (i) => { const [x, y] = wander(i, g.w, g.h, g.x, g.y); return page.mouse.move(x, y); });
await page.keyboard.up("Shift");

// the list of the plan
await page.evaluate(() => { [...document.querySelectorAll("button, a")].find((b) => (b.title || b.getAttribute("aria-label")) === "Show the list" || (b.title || "").includes("list")).click(); });
await sleep(1500);

const scrollable = () => page.evaluate(() => {
  const all = [...document.querySelectorAll("*")].filter((e) => e.scrollHeight > e.clientHeight + 50 && /auto|scroll/.test(getComputedStyle(e).overflowY));
  all.sort((a, b) => b.scrollHeight - a.scrollHeight);
  const e = all[0]; if (!e) return null; e.setAttribute("data-bench", "1");
  const r = e.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height, tall: e.scrollHeight };
});
const list = await scrollable();
if (list) {
  await page.mouse.move(list.x + 200, list.y + 200);
  await scenario("list wheel scroll", 80, () => page.mouse.wheel({ deltaY: 120 }));
  await scenario("list hover", 80, (i) => page.mouse.move(list.x + 50 + ((i * 41) % (list.w - 100)), list.y + 30 + ((i * 29) % (list.h - 60))));
}

// allocations and the calendar of availability
for (const [name, hash] of [["allocations", "#/allocations/user/00000000-0000-0000-0000-000000000001"], ["availability", "#/availability/user/me"]]) {
  if (name === "availability") {
    await page.evaluate(() => [...document.querySelectorAll("a")].find((a) => a.href.includes("#/availability")).click());
  } else {
    await page.evaluate((h) => { location.hash = h; }, hash);
  }
  await sleep(2500);
  await page.evaluate(() => document.querySelectorAll("[data-bench]").forEach((e) => e.removeAttribute("data-bench")));
  const area = await scrollable();
  const w = area ?? { x: 100, y: 150, w: 1000, h: 600 };
  await page.mouse.move(w.x + w.w / 2, w.y + w.h / 2);
  await scenario(`${name} hover`, 100, (i) => page.mouse.move(w.x + 30 + ((i * 47) % (w.w - 60)), w.y + 20 + ((i * 31) % (w.h - 40))));
  if (area) {
    await scenario(`${name} wheel scroll`, 80, () => page.mouse.wheel({ deltaY: 100 }));
    await scenario(`${name} sideways scroll`, 60, (i) => page.mouse.wheel({ deltaX: i % 2 ? 150 : -150 }));
  }
}

console.table(rows);
if (errors.length) console.log("errors:", errors);
await browser.close();
