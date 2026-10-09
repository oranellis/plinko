// End to end checks of the client in a real browser against a running server, see run.sh
import puppeteer from "puppeteer-core";

const base = process.env.PLINKO_URL ?? "http://127.0.0.1:3411";
const browser = await puppeteer.launch({ executablePath: process.env.CHROMIUM ?? "chromium", args: ["--no-sandbox"], headless: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const errors = [];
async function session(width = 1280, height = 800) {
  const context = await browser.createBrowserContext();
  const page = await context.newPage();
  await page.setViewport({ width, height });
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => { if (m.type() === "error" && !m.text().includes("401")) errors.push(m.text()); });
  return page;
}
const click = (page, label) => page.evaluate((label) => {
  const same = (b) => [b.textContent.trim(), b.title, b.getAttribute("aria-label")].includes(label);
  const b = [...document.querySelectorAll("button, a")].find((b) => same(b) && b.offsetParent !== null);
  if (!b) throw new Error("no button " + label); b.click();
}, label);
// the plus of the toolbar lists what can be added once it is clicked
const add = async (page, label) => { await click(page, "Add"); await click(page, label); };
const fill = async (page, selector, text) => {
  await page.focus(selector);
  await page.keyboard.down("Control"); await page.keyboard.press("A"); await page.keyboard.up("Control");
  await page.keyboard.type(text);
};
const text = (page) => page.evaluate(() => document.body.innerText.replace(/\n+/g, " | "));
async function login(page, email, password) {
  await page.goto(`${base}/#/plans`, { waitUntil: "networkidle0" });
  await page.waitForSelector("input[type=email]");
  await page.type("input[type=email]", email);
  await page.type("input[type=password]", password);
  await page.click("button[type=submit]");
  await page.waitForSelector(".shell");
  await sleep(800);
}
const expect = (cond, what) => { console.log(cond ? "ok  " : "FAIL", what); if (!cond) process.exitCode = 1; };

const a = await session();
await login(a, "admin@example.com", "password123");
await click(a, "New Plan"); await sleep(200);
await a.type(".modal input[type=text]", "Collab plan");
await a.click(".modal button[type=submit]");
await a.waitForSelector(".plan");
await add(a, "Add task"); await a.waitForSelector(".editor form");
await fill(a, ".editor input[type=text]", "Design");
await click(a, "Save"); await a.waitForFunction(() => !document.querySelector(".editor"));
await sleep(1000);

// tapping a row in the chart selects it, and its bar opens the editor
// a new task starts on the first working day after the plan starts, in the first or second lane
// depending on how far off that is, so try them until one opens it
const box = await a.evaluate(() => { const r = document.querySelector("canvas.gantt").getBoundingClientRect(); return { x: r.x, y: r.y }; });
for (let day = 1; day < 5 && !(await a.$(".contextbar")); day++) {
  for (const lane of [0, 1]) {
    if (await a.$(".contextbar")) break;
    await a.mouse.click(box.x + 34 * day + 17, box.y + 56 + 36 * lane + 18);
    await sleep(300);
  }
}
expect(await a.evaluate(() => !!document.querySelector(".contextbar")), "tapping a row in the chart shows its bar");
await click(a, "Edit");
expect(await a.evaluate(() => !!document.querySelector(".editor")), "the bar's pencil opens the editor");
await click(a, "Close");
// dragging pans, the wheel zooms: the day labels change
const before = await a.screenshot({ encoding: "base64" });
await a.mouse.move(box.x + 500, box.y + 200);
await a.mouse.down(); await a.mouse.move(box.x + 300, box.y + 200, { steps: 5 }); await a.mouse.up();
const panned = await a.screenshot({ encoding: "base64" });
expect(before !== panned, "dragging pans the chart");
await a.evaluate(() => document.querySelector("canvas.gantt").dispatchEvent(new WheelEvent("wheel", { deltaY: 400, ctrlKey: true, bubbles: true, clientX: 500, clientY: 300 })));
await sleep(200);
const zoomed = await a.screenshot({ encoding: "base64" });
expect(panned !== zoomed, "ctrl and the wheel zoom");

// a second window of the same person sees changes live
const b = await session();
await login(b, "admin@example.com", "password123");
await b.evaluate(() => document.querySelector("a.card").click());
await b.waitForSelector(".plan");
await click(b, "Show a list"); await sleep(500);
expect((await text(b)).includes("Design"), "a second window shows the task");
await click(a, "Show a list"); await sleep(300);
await a.evaluate(() => [...document.querySelectorAll("button.item")][0].click());
await a.waitForSelector(".editor form");
await fill(a, ".editor input[type=text]", "Design v2");
await click(a, "Save"); await sleep(1500);
expect((await text(b)).includes("Design v2"), "a rename in one window shows in the other without reloading");

// a member sees nothing until they are put in the plan
await a.goto(`${base}/#/people`); await sleep(600);
await click(a, "New User"); await a.waitForSelector(".modal form");
const inputs = await a.$$(".modal input");
await inputs[0].type("Mia Member"); await inputs[1].type("mia@example.com"); await inputs[3].type("mia-password-1");
await a.click(".modal button[type=submit]"); await sleep(1500);
const m = await session(390, 780);
await login(m, "mia@example.com", "mia-password-1");
expect((await text(m)).includes("not in any plans"), "a member with no plans sees none");
await a.evaluate(() => document.querySelector("a[href='#/']").click()); await sleep(400);
await a.evaluate(() => document.querySelector("a.card").click()); await a.waitForSelector(".plan");
await click(a, "Plan settings"); await sleep(500);
await a.evaluate(() => {
  const sel = [...document.querySelectorAll("section select")].find((s) => [...s.options].some((o) => o.textContent === "Mia Member"));
  sel.value = [...sel.options].find((o) => o.textContent === "Mia Member").value; sel.dispatchEvent(new Event("change", { bubbles: true }));
});
await click(a, "Add"); await sleep(1500);
expect((await text(m)).includes("Collab plan"), "adding the member to the plan shows it to them live");
console.log("errors:", errors);
await browser.close();
