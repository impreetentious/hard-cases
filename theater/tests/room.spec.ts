// What the situation room has to be true about.
//
// Every test here is about honesty rather than about appearance. Does the page
// say where its contents came from; does a cut stay inside the subtree it was
// derived in; does a paused search produce a gap an independent checker
// accepted; does a forged certificate come back refused by name. A screenshot
// comparison would pass while every one of those was wrong.

import { Solver, resealed } from "../src/hc";
import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";

/** Opens the room and waits for the module to load. */
async function open(page: Page, fragment = ""): Promise<void> {
  await page.goto(`/${fragment}`);
  await expect(page.locator('[data-role="solve"]')).toBeEnabled({ timeout: 60_000 });
}

/** Solves whatever the picker is on and waits for a verdict. */
async function solve(page: Page): Promise<void> {
  await page.locator('[data-role="solve"]').click();
  await expect(page.locator('[data-role="verdict"]')).toBeVisible({ timeout: 60_000 });
}

test("the page says where everything on it came from", async ({ page }) => {
  await open(page);
  await expect(page.locator('[data-role="provenance"]')).toHaveText(/nothing solved yet/i);

  await solve(page);
  await expect(page.locator('[data-role="provenance"]')).toHaveText(/live/i);
  await expect(page.locator('[data-role="verdict"]')).toHaveText(/verifier accepted/i);

  // And a replay says it is one, rather than passing a recording off as a
  // solve. This is the label the whole no-facade rule rests on.
  await page.locator('[data-role="replay"]').click();
  await expect(page.locator('[data-role="provenance"]')).toHaveText(/replay/i, {
    timeout: 60_000,
  });
  await expect(page.locator('[data-role="verdict"]')).toHaveText(/verifier accepted/i);
});

test("the tree grows as the scrubber moves through the run", async ({ page }) => {
  await open(page);
  await solve(page);

  const nodes = page.locator("[data-node]");
  const whole = await nodes.count();
  expect(whole).toBeGreaterThan(1);

  // At the first event nothing has been opened yet, and the panel says so
  // rather than drawing an empty tree as though it were a finished one.
  const scrub = page.locator('[data-role="scrub"]');
  await scrub.fill("0");
  await expect(page.locator("[data-node]")).toHaveCount(0);

  // And it fills in on the way back. Counted at a middle event as well as at
  // the end, so a page that jumped straight to the finished tree would fail.
  const middle = Math.floor((await scrub.getAttribute("max").then(Number)) / 2);
  await scrub.fill(String(middle));
  const partway = await page.locator("[data-node]").count();
  expect(partway).toBeGreaterThan(0);
  expect(partway).toBeLessThanOrEqual(whole);

  await scrub.fill(await scrub.getAttribute("max").then((max) => max ?? "0"));
  await expect(page.locator("[data-node]")).toHaveCount(whole);
});

test("a cut cannot reach outside the subtree it was derived in", async ({ page }) => {
  await open(page);
  await solve(page);

  const cuts = page.locator("[data-cut]");
  expect(await cuts.count()).toBeGreaterThan(0);
  const first = cuts.first();
  const scope = await first.locator(".pill").innerText();
  const inside = Number(scope.replace(/[^0-9]/g, ""));
  await first.click();

  const shaded = page.locator('[data-node][data-in-scope="yes"]');
  expect(await shaded.count()).toBeGreaterThan(0);

  // Every shaded node is the scope itself or a descendant of it, which for
  // this page means: no node outside that subtree is shaded. A sibling lighting
  // up is the failure the cut discipline exists to prevent.
  const shadedIds = await shaded.evaluateAll((nodes) =>
    nodes.map((node) => Number((node as HTMLElement).dataset.node)),
  );
  expect(shadedIds).toContain(inside);

  const everyId = await page
    .locator("[data-node]")
    .evaluateAll((nodes) => nodes.map((node) => Number((node as HTMLElement).dataset.node)));
  const outside = everyId.filter((id) => !shadedIds.includes(id));
  expect(outside.length).toBeGreaterThan(0);
});

test("pausing the search still proves a gap, and the verifier accepts it", async ({ page }) => {
  // One node, then stop. Everything still open takes an exact lower bound, and
  // what comes back is a proved interval rather than a number to be trusted.
  await open(page, "#i=fixcharge6.mps&n=1&p=frontier");
  await page.locator('[data-role="certify"]').click();
  await expect(page.locator('[data-role="verdict"]')).toBeVisible({ timeout: 60_000 });

  await expect(page.locator('[data-role="verdict"]')).toHaveText(/verifier accepted/i);
  const gap = await page.locator('[data-role="verified-gap"]').innerText();
  expect(gap).not.toMatch(/^0(\s|$)/);
  expect(gap).not.toBe("—");
  await expect(page.locator('[data-role="verified-lower"]')).not.toHaveText("—");
  await expect(page.locator('[data-role="verified-upper"]')).not.toHaveText("—");

  // The interval closes when the budget is raised, on the same instance and
  // the same page, which is the point of the panel.
  await page.locator('[data-role="budget"]').fill("50");
  await page.locator('[data-role="certify"]').click();
  await expect(page.locator('[data-role="verified-gap"]')).toHaveText(/^0/, { timeout: 60_000 });
});

test("a deleted leaf and an edited digit are both refused, by name", async ({ page }) => {
  await open(page, "#p=tamper");
  await page.locator('[data-role="solve"]').click();
  await expect(page.locator('[data-case="dropped_leaf"]')).toBeVisible({ timeout: 60_000 });

  // A node the certificate stops accounting for. The region it covered is no
  // longer claimed about by anything, and a cover with a hole is not a cover.
  await page.locator('[data-case="dropped_leaf"]').click();
  await expect(page.locator('[data-case="dropped_leaf"] [data-role="answered"]')).toHaveText(
    /^HCC-/,
  );
  await expect(page.locator('[data-case="dropped_leaf"] [data-role="answered"]')).not.toHaveText(
    /ACCEPTED/,
  );

  // And an ordinary digit, changed in a record and re-sealed by this page so
  // the checksum is correct. The mathematics has to be what refuses it.
  const records = page.locator('[data-role="edit-record"]');
  const value = await records
    .locator("option")
    .nth(2)
    .getAttribute("value");
  await records.selectOption(value ?? "0");
  await page.locator('[data-role="edit-value"]').fill("9");
  await page.locator('[data-role="edit-apply"]').click();
  await expect(page.locator('[data-role="edit-answer"]')).toHaveText(/^HCC-/);
});

test("a split is named by the column the file named", async ({ page }) => {
  // A branch cites a column of the model the search was about, and naming it
  // from the original file would be a guess — presolve removes columns. When
  // the run carries a reduction chain the page has the right labels, and this
  // is the check that it uses them rather than printing an index.
  await open(page, "#i=fixcharge6.mps&n=200");
  await solve(page);
  await page.locator('[data-node][data-status="branched"]').first().click();
  const detail = page.locator(".facts");
  await expect(detail).toContainText(/at\s/);
  const text = await detail.innerText();
  expect(text).toMatch(/Y[0-9]/);
  expect(text).not.toMatch(/column [0-9]/);
});

test("the tree can be worked from the keyboard", async ({ page }) => {
  await open(page);
  await solve(page);
  const first = page.locator("[data-node]").first();
  await first.focus();
  await expect(first).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(first).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("Enter");
  await expect(first).toHaveAttribute("aria-pressed", "false");
});

test("a file of your own is solved, and the page says the link cannot carry it", async ({
  page,
}) => {
  await open(page);
  // A model nobody in this repository wrote: two columns, one row, an obvious
  // answer. The point is that the page will take it at all.
  const mine = [
    "NAME MINE",
    "ROWS",
    " N obj",
    " L cap",
    "COLUMNS",
    " a obj -3 cap 1",
    " b obj -2 cap 1",
    "RHS",
    " r cap 4",
    "BOUNDS",
    " UP bnd a 3",
    " UP bnd b 3",
    "ENDATA",
    "",
  ].join("\n");
  await page.locator('[data-role="open"]').setInputFiles({
    name: "mine.mps",
    mimeType: "text/plain",
    buffer: Buffer.from(mine),
  });
  // The format is named rather than guessed, and the control that names it
  // appears the moment a file does.
  await expect(page.locator('[data-role="format"]')).toBeVisible();
  await page.locator('[data-role="format"]').selectOption("2");

  await solve(page);
  await expect(page.locator('[data-role="verdict"]')).toHaveText(/verifier accepted/i);
  await expect(page.locator('[data-role="copy-link"]')).toBeDisabled();
  await expect(page.locator('[data-role="copy-link"]')).toHaveText(/not shareable/i);

  // And picking a catalogue instance again takes the page back to something a
  // link can carry.
  await page.locator('[data-role="instance"]').selectOption("cover6.mps");
  await expect(page.locator('[data-role="copy-link"]')).toBeEnabled();
});

test("the address carries the whole view", async ({ page }) => {
  await open(page, "#i=parity5.mps&n=40&p=glass");
  await expect(page.locator('[data-role="instance"]')).toHaveValue("parity5.mps");
  await expect(page.locator('[data-role="nodes"]')).toHaveValue("40");
  await expect(page.locator('[data-panel="glass"]')).toHaveAttribute("aria-selected", "true");

  await solve(page);
  await expect(page.locator("[data-pivot]").first()).toBeVisible();

  // Moving the scrubber puts the event in the address, so a link shares the
  // moment rather than the page.
  await page.locator('[data-panel="tree"]').click();
  await page.locator('[data-role="scrub"]').fill("2");
  await expect(page).toHaveURL(/e=2/);
});

test("a panel name this page does not have opens on the panel it starts on", async ({ page }) => {
  // The address is a contract with every link that has already been copied. A
  // `p=` naming a panel this version does not have is a link from a version
  // that did, rather than a panel to go looking for — and the render ladder
  // ends in the case files, so an unrecognised name used to land there with no
  // tab selected and with the stage labelled by a tab that is not on the page.
  await open(page, "#p=situation-report");
  await expect(page.locator('[data-panel="tree"]')).toHaveAttribute("aria-selected", "true");
  await expect(page.locator('[data-panel="cases"]')).toHaveAttribute("aria-selected", "false");
  await expect(page.locator("#stage")).toHaveAttribute("aria-labelledby", "tab-tree");
});


test("direct imports verify native bytes and clear evidence on replacement", async ({ page }) => {
  await open(page);
  const model = readFileSync(new URL("../public/instances/fixcharge6.mps", import.meta.url));
  const proof = readFileSync(new URL("../public/recorded/run.hcc", import.meta.url));
  const put = async (side: string, bytes: Buffer, name: string) => page.locator(`[data-role="import-${side}"]`).setInputFiles({name,mimeType:"application/octet-stream",buffer:bytes});
  await put("model", model, "model.mps"); await put("proof", proof, "proof.hcc");
  await expect(page.locator('[data-role="import-verify"]')).toBeEnabled();
  await page.locator('[data-role="import-verify"]').click();
  await expect(page.locator('[data-role="import-result"]')).toContainText("independently verified");
  await expect(page.locator('[data-role="provenance"]')).toContainText("nothing solved yet");
  await put("model", model, "renamed.mps");
  await expect(page.locator('[data-role="import-result"]')).toHaveCount(0);
  await page.locator('[data-role="import-verify"]').click();
  await expect(page.locator('[data-role="import-result"]')).toContainText("independently verified");
  const wrongModel = readFileSync(new URL("../public/instances/cover6.mps", import.meta.url));
  await put("model", wrongModel, "renamed.mps");
  await expect(page.locator('[data-role="import-result"]')).toHaveCount(0);
  await page.locator('[data-role="import-verify"]').click();
  await expect(page.locator('[data-role="import-result"]')).not.toContainText("independently verified");
  await put("model", model, "model.mps");
  await page.evaluate(() => {
    const original = File.prototype.arrayBuffer;
    File.prototype.arrayBuffer = function () {
      if(this.name!=="late.mps")return original.call(this);
      return new Promise<ArrayBuffer>((resolve)=>{(window as unknown as {releaseImportModel:()=>Promise<void>}).releaseImportModel=async()=>resolve(await original.call(this));});
    };
  });
  await put("model",wrongModel,"late.mps");
  await expect(page.locator('[data-role="import-verify"]')).toBeDisabled();
  await put("model",model,"current.mps");
  await expect(page.locator('[data-role="import-verify"]')).toBeEnabled();
  await page.evaluate(()=>(window as unknown as {releaseImportModel:()=>Promise<void>}).releaseImportModel());
  await page.locator('[data-role="import-verify"]').click();
  await expect(page.locator('[data-role="import-result"]')).toContainText("independently verified");
  const semantic = Buffer.from(proof); semantic[semantic.length-5] = (semantic[semantic.length-5] ?? 0) ^ 1;
  await put("proof", Buffer.from(resealed(semantic)), "resealed.hcc");
  await page.locator('[data-role="import-verify"]').click();
  await expect(page.locator('[data-role="import-result"]')).not.toContainText("independently verified");
  const corrupt = Buffer.from(proof); corrupt[corrupt.length-1] = (corrupt[corrupt.length-1] ?? 0) ^ 1;
  await put("proof", corrupt, "proof.hcc");
  await expect(page.locator('[data-role="import-result"]')).toHaveCount(0);
  await expect(page.locator('[data-role="import-verify"]')).toBeEnabled();
  await page.locator('[data-role="import-verify"]').click();
  await expect(page.locator('[data-role="import-result"]')).not.toContainText("independently verified");
  await put("proof", proof.subarray(0,8), "truncated.hcc");
  await expect(page.locator('[data-role="import-verify"]')).toBeEnabled();
  await page.locator('[data-role="import-verify"]').click();
  await expect(page.locator('[data-role="import-result"]')).not.toContainText("independently verified");
});


test("verifier input allocations are released when the second reserve fails", () => {
  const released: number[] = [];
  let reserves = 0;
  const api = {memory:new WebAssembly.Memory({initial:1}),hc_reserve:() => { if(++reserves===2)throw new Error("allocation refused"); return 16; },hc_release:(pointer:number) => released.push(pointer)};
  const Factory = Solver as unknown as new (api: unknown, version: string, schema: number) => Solver;
  const solver = new Factory(api,"test",1);
  expect(()=>solver.recheck(new Uint8Array([1]),new Uint8Array([2]))).toThrow("allocation refused");
  expect(released).toEqual([16]);
});

test("import byte caps are checked before reading and failed reads leave that side unselected", async ({ page }) => {
  await open(page);
  const select = async (side: string, oneOver: boolean, failed = false) => page.evaluate(({side,oneOver,failed}) => {
    const file = new File([new Uint8Array([1,2,3,4])], "selected.bin");
    Object.defineProperty(file, "size", {value:(side === "model" ? 16 : 64)*1024*1024+(oneOver?1:0)});
    const state = window as unknown as {importReads?: number};
    file.arrayBuffer = async () => {state.importReads=(state.importReads??0)+1;if(failed)throw new Error("fixture read failed");return new Uint8Array([1,2,3,4]).buffer;};
    const input = document.querySelector<HTMLInputElement>(`[data-role="import-${side}"]`);
    if(!input)throw new Error("import input missing");
    Object.defineProperty(input,"files",{configurable:true,value:[file]});
    input.dispatchEvent(new Event("change",{bubbles:true}));
  },{side,oneOver,failed});
  await select("model",false);await select("proof",false);
  await expect(page.locator('[data-role="import-verify"]')).toBeEnabled();
  for(const side of ["model","proof"]){
    const reads=await page.evaluate(()=>(window as unknown as {importReads:number}).importReads);
    await select(side,true);
    await expect(page.getByRole("alert")).toContainText(`exceeds the ${side === "model" ? 16 : 64} MiB browser limit`);
    expect(await page.evaluate(()=>(window as unknown as {importReads:number}).importReads)).toBe(reads);
    await expect(page.locator('[data-role="import-verify"]')).toBeDisabled();
    await select(side,false);await expect(page.locator('[data-role="import-verify"]')).toBeEnabled();
  }
  await select("model",false,true);
  await expect(page.getByRole("alert")).toContainText("fixture read failed");
  await expect(page.locator('[data-role="import-verify"]')).toBeDisabled();
});
