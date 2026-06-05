const puppeteer = require('puppeteer');
const path = require('path');

const W = 1920;
const H = 1080;

(async () => {
  const browser = await puppeteer.launch({
    executablePath: '/usr/bin/google-chrome',
    args: [
      '--no-sandbox',
      '--disable-setuid-sandbox',
      '--disable-dev-shm-usage',
      '--disable-gpu',
      '--font-render-hinting=none',
    ],
    headless: 'new',
  });

  const page = await browser.newPage();

  // Set viewport to 16:9 slide dimensions
  await page.setViewport({ width: W, height: H, deviceScaleFactor: 2 });

  const filePath = path.resolve(__dirname, 'kask-pitch-deck.html');
  await page.goto(`file://${filePath}`, { waitUntil: 'networkidle0', timeout: 30000 });

  // Wait for fonts to load
  await page.waitForFunction(() => document.fonts.ready);
  await new Promise(r => setTimeout(r, 1500));

  // Inject print CSS: make each .slide exactly W×H, no scrolling
  await page.addStyleTag({ content: `
    * { -webkit-print-color-adjust: exact !important; print-color-adjust: exact !important; }
    html, body {
      width: ${W}px !important;
      background: #0a0a0a !important;
      margin: 0 !important;
      padding: 0 !important;
      overflow: visible !important;
    }
    .deck {
      max-width: ${W}px !important;
      width: ${W}px !important;
      margin: 0 !important;
    }
    .slide {
      width: ${W}px !important;
      height: ${H}px !important;
      min-height: ${H}px !important;
      max-height: ${H}px !important;
      overflow: hidden !important;
      page-break-after: always !important;
      break-after: page !important;
      padding: 56px 96px !important;
      box-sizing: border-box !important;
    }
  `});

  const outputPath = path.resolve(__dirname, 'kask-pitch-deck.pdf');

  await page.pdf({
    path: outputPath,
    width: `${W}px`,
    height: `${H}px`,
    printBackground: true,
    margin: { top: 0, right: 0, bottom: 0, left: 0 },
  });

  await browser.close();
  console.log(`PDF written to: ${outputPath}`);
})();
