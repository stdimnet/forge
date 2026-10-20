import { describe, expect, it } from "vitest";
import { detectLanguage, languageLabel, registerLanguage } from "./languages";

describe("language detection", () => {
  it.each([
    ["app/Models/User.php", "PHP"],
    ["resources/views/welcome.blade.php", "Blade"],
    ["composer.json", "JSON"],
    ["composer.lock", "JSON"],
    ["docker-compose.yml", "YAML"],
    ["config/services.yaml", "YAML"],
    ["database/schema.sql", "SQL"],
    [".env", "Dotenv"],
    [".env.example", "Dotenv"],
    ["deploy/prod.env", "Dotenv"],
    ["Dockerfile", "Dockerfile"],
    ["Dockerfile.dev", "Dockerfile"],
    ["README.md", "Markdown"],
    ["resources/js/app.ts", "TypeScript"],
    ["phpunit.xml", "XML"],
    ["phpunit.xml.dist", "XML"],
    ["Cargo.toml", "TOML"],
    [".gitignore", "Ignore File"],
    ["templates/order/show.html.twig", "Twig"],
    ["legacy/templates/cart.tpl", "Smarty"],
    ["app/Presenters/templates/Home.latte", "Latte"],
    ["templates/mail.hbs", "Handlebars"],
    ["src/App.vue", "Vue"],
    ["src/main.tsx", "TypeScript React"],
    ["styles/app.scss", "SCSS"],
    ["main.go", "Go"],
    ["script.py", "Python"],
    ["Gemfile", "Ruby"],
    ["Jenkinsfile", "Groovy"],
    ["Makefile", "Makefile"],
    [".htaccess", "Apache Config"],
    ["requests.http", "HTTP Request"],
    ["LICENSE", "Plain text"],
  ])("%s → %s", (path, label) => {
    expect(languageLabel(path)).toBe(label);
  });

  it("lets packs override a language", () => {
    registerLanguage({ id: "twig", label: "Twig (Symfony)", extensions: ["html.twig", "twig"], load: () => [] });
    expect(detectLanguage("templates/base.html.twig")?.label).toBe("Twig (Symfony)");
  });
});
