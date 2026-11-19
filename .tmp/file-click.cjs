const { chromium } = require('/home/sandy/.local/share/mise/installs/npm-playwright/latest/node_modules/playwright');

const html = `<!doctype html><html><body>
<label id="hiddenLabel">hidden<input id="hidden" type="file" style="display:none"></label>
<label id="clipLabel" style="position:relative;display:inline-block;width:40px;height:40px;background:#ccc">clip
<input id="clip" type="file" style="position:absolute;inset:0;opacity:0;width:100%;height:100%;"></label>
<input id="visible" type="file" style="display:inline-flex;appearance:none;height:40px;width:200px;overflow:clip">
</body></html>`;

(async () => {
  const browser = await chromium.launch({ executablePath: '/usr/bin/chromium', headless: true });
  const page = await browser.newPage();
  await page.setContent(html);
  const results = {};
  for (const id of ['hiddenLabel', 'clipLabel', 'visible']) {
    let opened = false;
    const waiter = page.waitForEvent('filechooser', { timeout: 1500 }).then(() => { opened = true; }).catch(() => {});
    await page.click('#' + id);
    await waiter;
    results[id] = opened;
  }
  console.log(JSON.stringify(results));
  await browser.close();
})();
