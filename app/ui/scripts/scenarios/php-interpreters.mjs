/** PHP CLI interpreters: Settings | PHP picker, the CLI Interpreters dialog, remote interpreters and runs. */

/** Demo interpreters: a detected local PHP, a Docker image and the compose service (project only). */
async function seed(page, selected = "app (Docker Compose)") {
  await page.evaluate(async (selected) => {
    const { saveSettings } = await import("/src/state/settings.ts");
    await saveSettings({
      "php.interpreters": [
        {
          id: "local-83",
          name: "PHP 8.3",
          kind: "local",
          path: "/usr/bin/php",
          version: "8.3.12",
          configFile: "/etc/php/8.3/cli/php.ini",
          additionalIni: "/etc/php/8.3/cli/conf.d/10-opcache.ini, /etc/php/8.3/cli/conf.d/20-xdebug.ini",
          xdebug: "3.4.1",
          extensions: "Core,ctype,mbstring,pdo_mysql,xdebug,Zend OPcache",
        },
        {
          id: "docker-82",
          name: "php:8.2-cli (Docker)",
          kind: "docker",
          image: "php:8.2-cli",
          version: "8.2.27",
          configFile: "/usr/local/etc/php/php.ini",
          extensions: "Core,ctype,mbstring",
        },
      ],
      "php.projectInterpreters": [
        {
          id: "compose-app",
          name: "app (Docker Compose)",
          kind: "compose",
          composeFiles: "docker-compose.yml",
          service: "app",
          lifecycle: "exec",
          path: "/usr/local/bin/php",
          options: "memory_limit=-1\nxdebug.start_with_request=trigger",
          version: "8.3.14",
          configFile: "/usr/local/etc/php/php.ini",
          xdebug: "3.4.1",
          extensions: "Core,ctype,mbstring,pdo_mysql,xdebug,Zend OPcache",
        },
      ],
      "php.interpreter": selected,
    });
  }, selected);
}

async function openPhpSettings(page) {
  await page.evaluate(async () => {
    const { openSettings } = await import("/src/features/settings/dialogState.ts");
    openSettings("lang.php");
  });
  await page.waitForSelector(".php-interp-picker", { timeout: 8000 });
}

async function openDialog(page, select = "app (Docker Compose)") {
  await seed(page);
  await openPhpSettings(page);
  await page.click('[data-testid="php-interpreters-open"]');
  await page.waitForSelector('[data-testid="php-interpreters-dialog"]');
  await page.click(`.php-interp-row[data-interpreter="${select}"]`);
  await page.waitForTimeout(400);
}

/** @type {Record<string, (page: import("playwright-core").Page) => Promise<void>>} */
export default {
  "php-interpreters-settings": async (page) => {
    await seed(page);
    await openPhpSettings(page);
    await page.waitForTimeout(400);
  },

  "php-interpreters-dialog": async (page) => {
    await openDialog(page);
    await page.waitForSelector('[data-testid="php-interp-mappings"] .php-interp-trow.auto');
    await page.waitForTimeout(200);
  },

  "php-interpreters-local": async (page) => {
    await openDialog(page, "PHP 8.3");
    await page.click('[data-testid="php-interp-add"]');
    await page.waitForSelector('[data-testid="php-interp-add-menu"] .php-interp-menu-sep');
    await page.waitForTimeout(200);
  },

  "php-interpreters-remote": async (page) => {
    await openDialog(page);
    await page.click('[data-testid="php-interp-add"]');
    await page.click('[data-testid="php-interp-add-menu"] button:has-text("From Docker")');
    await page.waitForSelector('[data-testid="php-remote-interpreter-dialog"]');
    await page.click('[data-testid="php-remote-interpreter-dialog"] label:has-text("Docker Compose")');
    await page.waitForTimeout(400);
  },

  "php-interpreters-phpinfo": async (page) => {
    await openDialog(page, "php:8.2-cli (Docker)");
    await page.click('[data-testid="php-interp-phpinfo"]');
    await page.waitForSelector(".php-phpinfo");
    await page.waitForTimeout(200);
  },

  "php-interpreters-run-config": async (page) => {
    await seed(page);
    await page.click(".run-config");
    await page.click(".run-dropdown .run-dd-item:has-text('Edit Configurations')");
    await page.waitForSelector(".rc-dialog");
    await page.click('.rc-tree-item[data-config="UserTest"]');
    await page.selectOption('[data-testid="rc-interpreter"]', "name:php:8.2-cli (Docker)");
    await page.waitForTimeout(500);
  },

  "php-interpreters-run": async (page) => {
    await seed(page);
    await page.click(".run-config");
    await page.waitForSelector(".run-dropdown");
    await page.hover('.run-dropdown .run-dd-item[data-config="artisan migrate"]');
    await page.click('.run-dropdown .run-dd-item[data-config="artisan migrate"] .run-btn');
    await page.waitForSelector(".run-console .run-line");
    await page.waitForTimeout(2600);
    // The command line is the console's first line.
    await page.evaluate(() => document.querySelectorAll(".run-console, .run-console *").forEach((el) => (el.scrollTop = 0)));
    await page.waitForTimeout(200);
  },

  "php-interpreters-dialog-light": async (page) => {
    await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
    await openDialog(page);
    await page.waitForSelector('[data-testid="php-interp-mappings"] .php-interp-trow.auto');
    await page.waitForTimeout(200);
  },
};
