/**
 * `?project=spring`: the Java demo `inventory-service` (demoJavaProject.ts) as a Spring Boot
 * 3 application: REST controllers, a service with constructor injection, Spring Data JPA
 * entities and a repository, `@Configuration` with `@Bean` methods, `@ConfigurationProperties`,
 * a JAX-RS resource through Jersey with CDI-style injection, and `application.yml` /
 * profile files. The analysis runs in Rust (WebAssembly, springMock.ts); the configuration
 * metadata of the "jars", the actuator's answers and the application's console are canned.
 */

/** True when the page asks for the Spring Boot demo. */
export const isSpringDemo = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("project") === "spring";

const PKG = "src/main/java/com/acme/inventory";

export const SPRING_BOOT_VERSION = "3.3.2";

export const springDemoFiles: Record<string, string> = {
  "build.gradle.kts": `plugins {
    java
    jacoco
    id("org.springframework.boot") version "${SPRING_BOOT_VERSION}"
    id("io.spring.dependency-management") version "1.1.6"
}

group = "com.acme"
version = "1.4.0"

java {
    toolchain {
        languageVersion = JavaLanguageVersion.of(21)
    }
}

repositories {
    mavenCentral()
}

dependencies {
    implementation("org.springframework.boot:spring-boot-starter-web")
    implementation("org.springframework.boot:spring-boot-starter-data-jpa")
    implementation("org.springframework.boot:spring-boot-starter-jersey")
    implementation("org.springframework.boot:spring-boot-starter-actuator")
    implementation("com.google.guava:guava:33.3.1-jre")
    runtimeOnly("com.h2database:h2")
    annotationProcessor("org.springframework.boot:spring-boot-configuration-processor")
    testImplementation("org.springframework.boot:spring-boot-starter-test")
    testImplementation(platform("org.junit:junit-bom:5.11.3"))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.test {
    useJUnitPlatform()
    finalizedBy(tasks.jacocoTestReport)
}

tasks.jacocoTestReport {
    reports {
        xml.required = true
    }
}
`,
  [`${PKG}/InventoryApplication.java`]: `package com.acme.inventory;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.boot.context.properties.ConfigurationPropertiesScan;

/** The inventory service: stock levels, warehouses and reorder alerts over HTTP. */
@SpringBootApplication
@ConfigurationPropertiesScan
public class InventoryApplication {

    public static void main(String[] args) {
        SpringApplication.run(InventoryApplication.class, args);
    }
}
`,
  [`${PKG}/web/StockController.java`]: `package com.acme.inventory.web;

import com.acme.inventory.stock.Product;
import com.acme.inventory.stock.StockService;
import java.util.List;
import org.springframework.http.MediaType;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping(path = ApiPaths.STOCK, produces = MediaType.APPLICATION_JSON_VALUE)
public class StockController {

    private final StockService stock;

    public StockController(StockService stock) {
        this.stock = stock;
    }

    @GetMapping
    public List<Product> list(@RequestParam(defaultValue = "false") boolean lowOnly) {
        return lowOnly ? stock.lowStock() : stock.all();
    }

    @GetMapping("/{sku}")
    public ResponseEntity<Product> find(@PathVariable String sku) {
        return ResponseEntity.of(stock.find(sku));
    }

    @PostMapping(consumes = MediaType.APPLICATION_JSON_VALUE)
    public Product receive(@RequestBody Delivery delivery) {
        return stock.receive(delivery.sku(), delivery.quantity());
    }

    @DeleteMapping("/{sku}/items/{quantity}")
    public Product ship(@PathVariable String sku, @PathVariable int quantity) {
        return stock.ship(sku, quantity);
    }

    record Delivery(String sku, int quantity) {}
}
`,
  [`${PKG}/web/ApiPaths.java`]: `package com.acme.inventory.web;

/** Base paths of the HTTP API. */
public final class ApiPaths {
    public static final String API = "/api";
    public static final String STOCK = API + "/stock";

    private ApiPaths() {}
}
`,
  [`${PKG}/web/ReportController.java`]: `package com.acme.inventory.web;

import com.acme.inventory.stock.StockService;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/api/reports")
public class ReportController {

    @Autowired
    private StockService stock;

    @GetMapping(value = "/low-stock", produces = "text/csv")
    public String lowStock() {
        return stock.lowStockCsv();
    }
}
`,
  [`${PKG}/stock/StockService.java`]: `package com.acme.inventory.stock;

import com.acme.inventory.config.InventoryProperties;
import java.time.Clock;
import java.util.List;
import java.util.Optional;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

@Service
@Transactional
public class StockService {

    private final ProductRepository products;
    private final List<StockListener> listeners;
    private final InventoryProperties properties;
    private final Clock clock;

    public StockService(ProductRepository products, List<StockListener> listeners, InventoryProperties properties, Clock clock) {
        this.products = products;
        this.listeners = listeners;
        this.properties = properties;
        this.clock = clock;
    }

    public List<Product> all() {
        return products.findAll();
    }

    public Optional<Product> find(String sku) {
        return products.findBySku(sku);
    }

    public List<Product> lowStock() {
        return products.findByQuantityLessThan(properties.lowStockThreshold());
    }

    public String lowStockCsv() {
        StringBuilder csv = new StringBuilder("sku,name,quantity\\n");
        for (Product p : lowStock()) {
            csv.append(p.getSku()).append(',').append(p.getName()).append(',').append(p.getQuantity()).append('\\n');
        }
        return csv.toString();
    }

    public Product receive(String sku, int quantity) {
        Product p = products.findBySku(sku).orElseThrow();
        p.setQuantity(p.getQuantity() + quantity);
        return products.save(p);
    }

    public Product ship(String sku, int quantity) {
        Product p = products.findBySku(sku).orElseThrow();
        p.setQuantity(p.getQuantity() - quantity);
        if (p.getQuantity() < properties.lowStockThreshold()) {
            listeners.forEach(l -> l.lowStock(p, clock.instant()));
        }
        return products.save(p);
    }
}
`,
  [`${PKG}/stock/StockListener.java`]: `package com.acme.inventory.stock;

import java.time.Instant;

/** Told when a product runs low. */
public interface StockListener {
    void lowStock(Product product, Instant at);
}
`,
  [`${PKG}/stock/ReorderMailer.java`]: `package com.acme.inventory.stock;

import com.acme.inventory.config.InventoryProperties;
import java.time.Instant;
import org.springframework.stereotype.Component;

@Component
public class ReorderMailer implements StockListener {

    private final InventoryProperties properties;

    public ReorderMailer(InventoryProperties properties) {
        this.properties = properties;
    }

    @Override
    public void lowStock(Product product, Instant at) {
        System.out.printf("Reorder %s, mail to %s%n", product.getSku(), properties.reorderEmail());
    }
}
`,
  [`${PKG}/stock/AuditLog.java`]: `package com.acme.inventory.stock;

import java.time.Instant;
import org.springframework.stereotype.Component;

@Component
public class AuditLog implements StockListener {

    @Override
    public void lowStock(Product product, Instant at) {
        System.out.printf("%s low on stock at %s%n", product.getSku(), at);
    }
}
`,
  [`${PKG}/stock/ProductRepository.java`]: `package com.acme.inventory.stock;

import java.util.List;
import java.util.Optional;
import org.springframework.data.jpa.repository.JpaRepository;

public interface ProductRepository extends JpaRepository<Product, Long> {

    Optional<Product> findBySku(String sku);

    List<Product> findByQuantityLessThan(int threshold);
}
`,
  [`${PKG}/stock/Product.java`]: `package com.acme.inventory.stock;

import jakarta.persistence.*;

@Entity
@Table(name = "products")
public class Product {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @Column(name = "sku", unique = true, nullable = false)
    private String sku;

    private String name;

    private int quantity;

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(name = "warehouse_id")
    private Warehouse warehouse;

    public String getSku() { return sku; }
    public String getName() { return name; }
    public int getQuantity() { return quantity; }
    public void setQuantity(int quantity) { this.quantity = quantity; }
}
`,
  [`${PKG}/stock/Warehouse.java`]: `package com.acme.inventory.stock;

import jakarta.persistence.*;
import java.util.List;

@Entity
public class Warehouse {

    @Id
    @GeneratedValue
    private Long id;

    private String city;

    @OneToMany(mappedBy = "warehouse")
    private List<Product> products;
}
`,
  [`${PKG}/config/InventoryConfig.java`]: `package com.acme.inventory.config;

import java.time.Clock;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.context.annotation.Profile;

@Configuration
public class InventoryConfig {

    @Bean
    public Clock clock() {
        return Clock.systemUTC();
    }

    @Bean
    @Profile("dev")
    public DemoData demoData(com.acme.inventory.stock.ProductRepository products) {
        return new DemoData(products);
    }
}
`,
  [`${PKG}/config/DemoData.java`]: `package com.acme.inventory.config;

import com.acme.inventory.stock.ProductRepository;

/** Sample products for the dev profile. */
public class DemoData {
    public DemoData(ProductRepository products) {}
}
`,
  [`${PKG}/config/InventoryProperties.java`]: `package com.acme.inventory.config;

import org.springframework.boot.context.properties.ConfigurationProperties;

/** Settings of the inventory service. */
@ConfigurationProperties(prefix = "inventory")
public record InventoryProperties(
    /** Products below this quantity are reported as low on stock. */
    int lowStockThreshold,
    /** Where reorder mails go. */
    String reorderEmail,
    /** How stock changes are booked. */
    BookingMode booking
) {
    public enum BookingMode { IMMEDIATE, BATCHED }
}
`,
  [`${PKG}/legacy/JerseyConfig.java`]: `package com.acme.inventory.legacy;

import jakarta.ws.rs.ApplicationPath;
import org.glassfish.jersey.server.ResourceConfig;
import org.springframework.stereotype.Component;

/** The JAX-RS API of the old warehouse clients, served by Jersey under /rest. */
@Component
@ApplicationPath("/rest")
public class JerseyConfig extends ResourceConfig {
    public JerseyConfig() {
        register(WarehouseResource.class);
    }
}
`,
  [`${PKG}/legacy/WarehouseResource.java`]: `package com.acme.inventory.legacy;

import com.acme.inventory.stock.Product;
import com.acme.inventory.stock.StockService;
import jakarta.inject.Inject;
import jakarta.ws.rs.*;
import jakarta.ws.rs.core.MediaType;
import java.util.List;

@Path("/warehouses")
@Produces(MediaType.APPLICATION_JSON)
public class WarehouseResource {

    @Inject
    StockService stock;

    @GET
    @Path("{city}/stock")
    public List<Product> stock(@PathParam("city") String city, @QueryParam("low") boolean low) {
        return low ? stock.lowStock() : stock.all();
    }

    @POST
    @Path("{city}/inventory")
    @Consumes(MediaType.APPLICATION_JSON)
    public void count(@PathParam("city") String city, List<Product> counted) {}
}
`,
  "src/main/resources/application.yml": `spring:
  application:
    name: inventory-service
  datasource:
    url: jdbc:h2:mem:inventory
    username: sa
  jpa:
    open-in-view: false
    hibernate:
      ddl-auto: update
    show-sql: true
  main:
    banner-mode: console

server:
  port: 8080
  shutdown: graceful

management:
  endpoints:
    web:
      exposure:
        include: health,info,beans,mappings
  endpoint:
    health:
      show-details: always

inventory:
  low-stock-threshold: 5
  reorder-mail: purchasing@acme.example
  booking: immediate

logging:
  level:
    com.acme.inventory: debug
`,
  "src/main/resources/application-dev.properties": `spring.h2.console.enabled=true
spring.jpa.show-sql=true
logging.level.org.hibernate.SQL=debug
`,
  "src/main/resources/application-prod.yml": `server:
  port: 80
spring:
  datasource:
    url: jdbc:postgresql://db.acme.internal/inventory
  jpa:
    show-sql: false
`,
  "requests/stock.http": `### All products
GET http://localhost:8080/api/stock
Accept: application/json
`,
};

/** `spring-configuration-metadata.json` of the demo's "jars" (an excerpt of Spring Boot's). */
export const springDemoMetadata: [string, string][] = [
  [
    `spring-boot-autoconfigure-${SPRING_BOOT_VERSION}.jar`,
    JSON.stringify({
      groups: [
        { name: "server", type: "org.springframework.boot.autoconfigure.web.ServerProperties", sourceType: "org.springframework.boot.autoconfigure.web.ServerProperties" },
        { name: "spring.datasource", type: "org.springframework.boot.autoconfigure.jdbc.DataSourceProperties", sourceType: "org.springframework.boot.autoconfigure.jdbc.DataSourceProperties" },
        { name: "spring.jpa", type: "org.springframework.boot.autoconfigure.orm.jpa.JpaProperties", sourceType: "org.springframework.boot.autoconfigure.orm.jpa.JpaProperties" },
      ],
      properties: [
        { name: "server.port", type: "java.lang.Integer", description: "Server HTTP port.", sourceType: "org.springframework.boot.autoconfigure.web.ServerProperties", defaultValue: 8080 },
        { name: "server.address", type: "java.net.InetAddress", description: "Network address to which the server should bind.", sourceType: "org.springframework.boot.autoconfigure.web.ServerProperties" },
        { name: "server.servlet.context-path", type: "java.lang.String", description: "Context path of the application.", sourceType: "org.springframework.boot.autoconfigure.web.ServerProperties$Servlet" },
        { name: "server.shutdown", type: "org.springframework.boot.web.server.Shutdown", description: "Type of shutdown that the server will support.", sourceType: "org.springframework.boot.autoconfigure.web.ServerProperties", defaultValue: "immediate" },
        { name: "server.compression.enabled", type: "java.lang.Boolean", description: "Whether response compression is enabled.", defaultValue: false },
        { name: "server.error.include-message", type: "org.springframework.boot.autoconfigure.web.ErrorProperties$IncludeAttribute", description: "When to include the \"message\" attribute.", defaultValue: "never" },
        { name: "server.max-http-header-size", type: "org.springframework.util.unit.DataSize", description: "Maximum size of the HTTP message header.", deprecated: true, deprecation: { level: "error", reason: "Use the request header size instead.", replacement: "server.max-http-request-header-size" } },
        { name: "server.max-http-request-header-size", type: "org.springframework.util.unit.DataSize", description: "Maximum size of the HTTP request header.", defaultValue: "8KB" },
        { name: "spring.application.name", type: "java.lang.String", description: "Application name.", sourceType: "org.springframework.boot.context.ContextIdApplicationContextInitializer" },
        { name: "spring.datasource.url", type: "java.lang.String", description: "JDBC URL of the database.", sourceType: "org.springframework.boot.autoconfigure.jdbc.DataSourceProperties" },
        { name: "spring.datasource.username", type: "java.lang.String", description: "Login username of the database.", sourceType: "org.springframework.boot.autoconfigure.jdbc.DataSourceProperties" },
        { name: "spring.datasource.password", type: "java.lang.String", description: "Login password of the database.", sourceType: "org.springframework.boot.autoconfigure.jdbc.DataSourceProperties" },
        { name: "spring.datasource.driver-class-name", type: "java.lang.String", description: "Fully qualified name of the JDBC driver. Auto-detected based on the URL by default.", sourceType: "org.springframework.boot.autoconfigure.jdbc.DataSourceProperties" },
        { name: "spring.jpa.show-sql", type: "java.lang.Boolean", description: "Whether to enable logging of SQL statements.", sourceType: "org.springframework.boot.autoconfigure.orm.jpa.JpaProperties", defaultValue: false },
        { name: "spring.jpa.open-in-view", type: "java.lang.Boolean", description: "Register OpenEntityManagerInViewInterceptor. Binds a JPA EntityManager to the thread for the entire processing of the request.", sourceType: "org.springframework.boot.autoconfigure.orm.jpa.JpaProperties", defaultValue: true },
        { name: "spring.jpa.hibernate.ddl-auto", type: "java.lang.String", description: "DDL mode. This is actually a shortcut for the \"hibernate.hbm2ddl.auto\" property. Defaults to \"create-drop\" when using an embedded database and no schema manager was detected. Otherwise, defaults to \"none\".", sourceType: "org.springframework.boot.autoconfigure.orm.jpa.HibernateProperties" },
        { name: "spring.jpa.database-platform", type: "java.lang.String", description: "Name of the target database to operate on, auto-detected by default.", sourceType: "org.springframework.boot.autoconfigure.orm.jpa.JpaProperties" },
        { name: "spring.h2.console.enabled", type: "java.lang.Boolean", description: "Whether to enable the console.", defaultValue: false },
        { name: "spring.sql.init.mode", type: "org.springframework.boot.sql.init.DatabaseInitializationMode", description: "Mode to apply when determining whether initialization should be performed.", defaultValue: "embedded" },
        { name: "spring.jersey.application-path", type: "java.lang.String", description: "Path that serves as the base URI for the application. If specified, overrides the value of \"@ApplicationPath\"." },
        { name: "spring.mvc.servlet.path", type: "java.lang.String", description: "Path of the dispatcher servlet.", defaultValue: "/" },
        { name: "spring.threads.virtual.enabled", type: "java.lang.Boolean", description: "Whether to use virtual threads.", defaultValue: false },
        { name: "spring.jackson.serialization", type: "java.util.Map<com.fasterxml.jackson.databind.SerializationFeature,java.lang.Boolean>", description: "Jackson on/off features that affect the way Java objects are serialized." },
      ],
      hints: [
        {
          name: "spring.jpa.hibernate.ddl-auto",
          values: [
            { value: "none", description: "Disable DDL handling." },
            { value: "validate", description: "Validate the schema, make no changes to the database." },
            { value: "update", description: "Update the schema if necessary." },
            { value: "create", description: "Create the schema and destroy previous data." },
            { value: "create-drop", description: "Create and then destroy the schema at the end of the session." },
          ],
        },
      ],
    }),
  ],
  [
    `spring-boot-${SPRING_BOOT_VERSION}.jar`,
    JSON.stringify({
      properties: [
        { name: "spring.main.banner-mode", type: "org.springframework.boot.Banner$Mode", description: "Mode used to display the banner when the application runs.", sourceType: "org.springframework.boot.SpringApplication", defaultValue: "console" },
        { name: "spring.main.lazy-initialization", type: "java.lang.Boolean", description: "Whether initialization should be performed lazily.", defaultValue: false },
        { name: "spring.profiles.active", type: "java.util.List<java.lang.String>", description: "Comma-separated list of active profiles. Can be overridden by a command line switch." },
        { name: "spring.profiles.include", type: "java.util.List<java.lang.String>", description: "Unconditionally activate the specified comma-separated list of profiles (or list of profiles if using YAML)." },
        { name: "spring.config.activate.on-profile", type: "java.lang.String[]", description: "Profile expressions that should match for the document to be included." },
        { name: "logging.level", type: "java.util.Map<java.lang.String,org.springframework.boot.logging.LogLevel>", description: "Log levels severity mapping. For instance, `logging.level.org.springframework=DEBUG`." },
        { name: "logging.file.name", type: "java.lang.String", description: "Log file name (for instance, `myapp.log`). Names can be an exact location or relative to the current directory." },
        { name: "logging.pattern.console", type: "java.lang.String", description: "Appender pattern for output to the console." },
      ],
      hints: [
        { name: "logging.level.keys", values: [{ value: "root", description: "Root logger used to assign the default logging level." }, { value: "sql", description: "SQL logging group including Hibernate SQL logger." }, { value: "web", description: "Web logging group including codecs." }] },
      ],
    }),
  ],
  [
    `spring-boot-actuator-autoconfigure-${SPRING_BOOT_VERSION}.jar`,
    JSON.stringify({
      properties: [
        { name: "management.endpoints.web.exposure.include", type: "java.util.Set<java.lang.String>", description: "Endpoint IDs that should be included or '*' for all." },
        { name: "management.endpoints.web.exposure.exclude", type: "java.util.Set<java.lang.String>", description: "Endpoint IDs that should be excluded or '*' for all." },
        { name: "management.endpoints.web.base-path", type: "java.lang.String", description: "Base path for Web endpoints. Relative to the servlet context path (server.servlet.context-path) or WebFlux base path (spring.webflux.base-path) when the management server is sharing the main server port.", defaultValue: "/actuator" },
        { name: "management.endpoint.health.show-details", type: "org.springframework.boot.actuate.endpoint.Show", description: "When to show full health details.", defaultValue: "never" },
        { name: "management.server.port", type: "java.lang.Integer", description: "Management endpoint HTTP port (uses the same port as the application by default). Configure a different port to use management-specific SSL." },
      ],
      hints: [{ name: "management.endpoints.web.exposure.include", values: [{ value: "*", description: "All endpoints." }, { value: "health" }, { value: "info" }, { value: "beans" }, { value: "mappings" }, { value: "env" }, { value: "metrics" }] }],
    }),
  ],
];

/** Enum constants of the library types the metadata uses (read from class files on the desktop). */
export const springDemoEnums: Record<string, string[]> = {
  "org.springframework.boot.Banner$Mode": ["OFF", "CONSOLE", "LOG"],
  "org.springframework.boot.web.server.Shutdown": ["GRACEFUL", "IMMEDIATE"],
  "org.springframework.boot.autoconfigure.web.ErrorProperties$IncludeAttribute": ["NEVER", "ALWAYS", "ON_PARAM"],
  "org.springframework.boot.sql.init.DatabaseInitializationMode": ["ALWAYS", "EMBEDDED", "NEVER"],
  "org.springframework.boot.actuate.endpoint.Show": ["NEVER", "WHEN_AUTHORIZED", "ALWAYS"],
  "org.springframework.boot.logging.LogLevel": ["TRACE", "DEBUG", "INFO", "WARN", "ERROR", "FATAL", "OFF"],
};

/** The console of `InventoryApplication` starting with the dev profile. */
export function springDemoConsole(profiles: string[], port: number, pid: number): string[] {
  const t = (ms: number) => `2026-10-27T09:41:0${Math.min(9, Math.floor(ms / 1000))}.${String(ms % 1000).padStart(3, "0")}+01:00`;
  const p = profiles.length ? profiles : ["default"];
  const active = profiles.length
    ? `The following ${p.length} profile${p.length > 1 ? "s are" : " is"} active: ${p.map((x) => `"${x}"`).join(", ")}`
    : 'No active profile set, falling back to 1 default profile: "default"';
  return [
    "",
    "  .   ____          _            __ _ _",
    " /\\\\ / ___'_ __ _ _(_)_ __  __ _ \\ \\ \\ \\",
    "( ( )\\___ | '_ | '_| | '_ \\/ _` | \\ \\ \\ \\",
    " \\\\/  ___)| |_)| | | | | || (_| |  ) ) ) )",
    "  '  |____| .__|_| |_|_| |_\\__, | / / / /",
    " =========|_|==============|___/=/_/_/_/",
    "",
    ` :: Spring Boot ::                (v${SPRING_BOOT_VERSION})`,
    "",
    `${t(120)}  INFO ${pid} --- [inventory-service] [           main] c.acme.inventory.InventoryApplication    : Starting InventoryApplication using Java 21.0.4 with PID ${pid} (/home/anna/Projects/inventory-service/build/classes/java/main started by anna in /home/anna/Projects/inventory-service)`,
    `${t(124)}  INFO ${pid} --- [inventory-service] [           main] c.acme.inventory.InventoryApplication    : ${active}`,
    `${t(880)}  INFO ${pid} --- [inventory-service] [           main] .s.d.r.c.RepositoryConfigurationDelegate : Finished Spring Data repository scanning in 41 ms. Found 1 JPA repository interface.`,
    `${t(1420)}  INFO ${pid} --- [inventory-service] [           main] o.s.b.w.embedded.tomcat.TomcatWebServer  : Tomcat initialized with port ${port} (http)`,
    `${t(1902)}  INFO ${pid} --- [inventory-service] [           main] com.zaxxer.hikari.HikariDataSource       : HikariPool-1 - Start completed.`,
    `${t(2210)}  INFO ${pid} --- [inventory-service] [           main] o.h.e.t.j.p.i.JtaPlatformInitiator       : HHH000489: No JTA platform available (set 'hibernate.transaction.jta.platform' to enable JTA platform integration)`,
    `${t(2650)}  INFO ${pid} --- [inventory-service] [           main] o.s.b.a.e.web.EndpointLinksResolver      : Exposing 4 endpoints beneath base path '/actuator'`,
    `${t(2790)}  INFO ${pid} --- [inventory-service] [           main] o.s.b.w.embedded.tomcat.TomcatWebServer  : Tomcat started on port ${port} (http) with context path '/'`,
    `${t(2801)}  INFO ${pid} --- [inventory-service] [           main] c.acme.inventory.InventoryApplication    : Started InventoryApplication in 2.874 seconds (process running for 3.312)`,
  ];
}

/** What the actuator of the running demo answers. */
export const springDemoActuator = {
  health: JSON.stringify({
    status: "UP",
    components: {
      db: { status: "UP", details: { database: "H2", validationQuery: "isValid()" } },
      diskSpace: { status: "UP", details: { total: 499963174912, free: 91300814848, threshold: 10485760, path: "/home/anna/Projects/inventory-service/.", exists: true } },
      ping: { status: "UP" },
    },
  }),
  beans: JSON.stringify({
    contexts: {
      "inventory-service": {
        beans: Object.fromEntries(
          [
            ["inventoryApplication", "com.acme.inventory.InventoryApplication", []],
            ["stockController", "com.acme.inventory.web.StockController", ["stockService"]],
            ["reportController", "com.acme.inventory.web.ReportController", ["stockService"]],
            ["stockService", "com.acme.inventory.stock.StockService", ["productRepository", "reorderMailer", "auditLog", "inventory-com.acme.inventory.config.InventoryProperties", "clock"]],
            ["reorderMailer", "com.acme.inventory.stock.ReorderMailer", ["inventory-com.acme.inventory.config.InventoryProperties"]],
            ["auditLog", "com.acme.inventory.stock.AuditLog", []],
            ["productRepository", "jdk.proxy2.$Proxy112", ["jpa.named-queries#0", "jpaSharedEM_entityManagerFactory"]],
            ["inventoryConfig", "com.acme.inventory.config.InventoryConfig", []],
            ["clock", "java.time.Clock$SystemClock", []],
            ["demoData", "com.acme.inventory.config.DemoData", ["productRepository"]],
            ["jerseyConfig", "com.acme.inventory.legacy.JerseyConfig", []],
            ["inventory-com.acme.inventory.config.InventoryProperties", "com.acme.inventory.config.InventoryProperties", []],
            ["entityManagerFactory", "org.springframework.orm.jpa.LocalContainerEntityManagerFactoryBean", ["dataSource"]],
            ["dataSource", "com.zaxxer.hikari.HikariDataSource", []],
            ["jacksonObjectMapper", "com.fasterxml.jackson.databind.ObjectMapper", []],
            ["transactionManager", "org.springframework.orm.jpa.JpaTransactionManager", ["entityManagerFactory"]],
            ["tomcatServletWebServerFactory", "org.springframework.boot.web.embedded.tomcat.TomcatServletWebServerFactory", []],
            ["healthEndpoint", "org.springframework.boot.actuate.health.HealthEndpoint", []],
          ].map(([name, type, deps]) => [name, { aliases: [], scope: "singleton", type, dependencies: deps }]),
        ),
        parentId: null,
      },
    },
  }),
  mappings: JSON.stringify({
    contexts: {
      "inventory-service": {
        mappings: {
          dispatcherServlets: {
            dispatcherServlet: [
              ["GET", "/api/stock", "com.acme.inventory.web.StockController", "list"],
              ["GET", "/api/stock/{sku}", "com.acme.inventory.web.StockController", "find"],
              ["POST", "/api/stock", "com.acme.inventory.web.StockController", "receive"],
              ["DELETE", "/api/stock/{sku}/items/{quantity}", "com.acme.inventory.web.StockController", "ship"],
              ["GET", "/api/reports/low-stock", "com.acme.inventory.web.ReportController", "lowStock"],
              ["GET", "/actuator", null, "Actuator root web endpoint"],
              ["GET", "/actuator/health", null, "Actuator web endpoint 'health'"],
              ["GET", "/actuator/beans", null, "Actuator web endpoint 'beans'"],
              ["GET", "/actuator/mappings", null, "Actuator web endpoint 'mappings'"],
              [null, "/error", "org.springframework.boot.autoconfigure.web.servlet.error.BasicErrorController", "error"],
            ].map(([method, pattern, cls, name]) => ({
              handler: cls ? `${cls}#${name}()` : name,
              predicate: `{${method ?? ""} [${pattern}]}`,
              details: {
                handlerMethod: cls ? { className: cls, name, descriptor: "()V" } : null,
                requestMappingConditions: { consumes: [], headers: [], methods: method ? [method] : [], params: [], patterns: [pattern], produces: [] },
              },
            })),
          },
          servletFilters: [],
          servlets: [],
        },
        parentId: null,
      },
    },
  }),
};
