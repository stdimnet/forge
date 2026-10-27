/**
 * Spring Boot and Jakarta EE (`forge.spring`, JavaForge), in the style of IntelliJ IDEA
 * Ultimate. Everything is computed in Rust (`packs/javaforge/fw-spring`,
 * `commands/spring.rs`); this module plugs it into the contribution points:
 *
 * - the Endpoints tool window (Spring MVC / JAX-RS endpoints, Open in HTTP Client) and the
 *   Spring tool window (Beans, Persistence, Configuration);
 * - gutter icons: beans ↔ injection points, endpoints, JPA entities ↔ repositories,
 *   `@ConfigurationProperties` classes, ▶ on `@SpringBootApplication` (`editor.ts`,
 *   `runType.tsx`);
 * - `application*.properties` / `application*.yml`: completion with documentation,
 *   inspections, hover and Ctrl+B to the `@ConfigurationProperties` field;
 * - the Spring Boot run configuration and its run dashboard in Services (`dashboard.ts`);
 * - Settings | Languages & Frameworks | Java | Spring Boot.
 *
 * Active only for projects whose build uses Spring Boot or Jakarta EE.
 */
import { createEffect, createRoot, on } from "solid-js";
import { Leaf } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerEndpointsView, showEndpointsView } from "@/features/endpoints/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerCommand } from "@/state/commands";
import { onProjectDetected } from "@/state/project";
import { showToolWindow } from "@/state/layout";
import { isDialogWindow } from "@/shell/window/mode";
import { EndpointsActions, EndpointsView } from "./EndpointsView";
import { setSpringTab, SpringActions, SpringView } from "./SpringView";
import { loadSpring, springActive } from "./store";
import "./editor";
import "./runType";
import "./dashboard";
import "./spring.css";

// The shared Endpoints tool window (features/endpoints) shows this view in Spring / Jakarta EE projects.
registerEndpointsView({ id: "spring", title: "Spring and JAX-RS", order: 20, when: springActive, component: EndpointsView, actions: EndpointsActions });

registerToolWindow({
  id: "spring",
  title: "Spring",
  icon: Leaf,
  anchor: "right",
  order: 45,
  component: SpringView,
  actions: SpringActions,
  when: springActive,
});

registerSettingsPage({
  id: "lang.java.spring-boot",
  title: "Spring Boot",
  group: "Languages & Frameworks",
  parent: "lang.java",
  order: 10,
  keywords: ["spring", "spring boot", "jakarta", "beans", "endpoints", "application.yml", "application.properties", "configuration metadata", "actuator", "run dashboard", "services", "jax-rs", "cdi", "jpa"],
  component: () => <SchemaPage pageId="lang.java.spring-boot" />,
});

registerCommand({ id: "spring.refresh", title: "Refresh Spring Model", category: "Spring", run: () => void loadSpring(true) });
registerCommand({ id: "spring.showEndpoints", title: "Endpoints", category: "View", run: () => showEndpointsView("spring") });
registerCommand({
  id: "spring.showBeans",
  title: "Spring Beans",
  category: "View",
  run: () => {
    setSpringTab("beans");
    showToolWindow("spring");
  },
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  onProjectDetected(() => {
    if (springActive()) void loadSpring();
  });
  createRoot(() => createEffect(on(springActive, (on) => on && void loadSpring())));
}
