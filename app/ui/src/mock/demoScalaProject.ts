/**
 * A small sbt build for browser mode, opened with `?project=scala`: a Scala 3 `core`
 * library with ScalaTest and MUnit suites, a Scala 2.13 `api` service and sbt plugins. The
 * build model, main objects, test discovery and command lines are computed by the Rust
 * `javaforge-lang-scala` crate through WebAssembly (jvmExtrasMock.ts); sbt's import output
 * is recorded.
 */
import type { ProjectInfo } from "@/ipc/project";

export const scalaDemoName = "shop";

/** True when the page asks for the Scala demo project. */
export const isScalaDemo = (): boolean =>
  typeof location !== "undefined" && new URLSearchParams(location.search).get("project") === "scala";

export const scalaDemoInfo: ProjectInfo = {
  languages: ["scala"],
  frameworks: [
    { id: "scalatest", name: "ScalaTest", language: "scala" },
    { id: "munit", name: "MUnit", language: "scala" },
  ],
  packs: ["javaforge"],
  tools: ["git", "sbt"],
  primary: { id: "scala", name: "Scala", version: "3.3", language: "scala" },
};

export const scalaDemoStartFile = "core/src/main/scala/com/acme/shop/Cart.scala";

export const scalaDemoFiles: Record<string, string> = {
  "build.sbt": `// The shop: a core library, an HTTP API and an aggregating root.
val scala3Version = "3.3.4"
val catsVersion = "2.12.0"

ThisBuild / organization := "com.acme"
ThisBuild / version      := "0.4.0-SNAPSHOT"
ThisBuild / scalaVersion := scala3Version

lazy val commonSettings = Seq(
  libraryDependencies += "org.scalameta" %% "munit" % "1.0.2" % Test
)

lazy val root = (project in file("."))
  .aggregate(core, api)
  .settings(
    name := "shop",
    publish / skip := true
  )

lazy val core = project
  .settings(
    commonSettings,
    name := "shop-core",
    libraryDependencies ++= Seq(
      "org.typelevel" %% "cats-core" % catsVersion,
      "org.scalatest" %% "scalatest" % "3.2.19" % "test"
    )
  )

lazy val api = project.in(file("modules/api"))
  .dependsOn(core % "compile->compile;test->test")
  .settings(
    commonSettings,
    name := "shop-api",
    scalaVersion := "2.13.15",
    libraryDependencies += "com.softwaremill.sttp.tapir" %% "tapir-core" % "1.11.7",
    libraryDependencies += "ch.qos.logback" % "logback-classic" % "1.5.12" % Runtime
  )
`,
  "project/build.properties": `sbt.version=1.10.5
`,
  "project/plugins.sbt": `addSbtPlugin("org.scalameta" % "sbt-scalafmt" % "2.5.2")
addSbtPlugin("com.github.sbt" % "sbt-native-packager" % "1.10.4")
`,
  ".scalafmt.conf": `version = "3.8.3"
runner.dialect = scala3
maxColumn = 100
`,
  ".gitignore": `target/
.bsp/
.bloop/
.metals/
.idea/
`,
  "README.md": `# shop

A small sbt build: the \`core\` library (Scala 3) and the \`api\` service (Scala 2.13).

\`\`\`sh
sbt test
sbt api/run
\`\`\`
`,
  "core/src/main/scala/com/acme/shop/Cart.scala": `package com.acme.shop

import cats.syntax.all.*

/** A line of the cart: an item and its quantity. */
final case class Item(name: String, cents: Long):
  require(cents >= 0, s"negative price for $name")

final case class Cart(items: List[Item]):
  def add(item: Item): Cart = copy(items = item :: items)

  def total: Long = items.map(_.cents).combineAll

  def fill(count: Int): Cart = Cart(List.fill(count)(Item("apple", 120)))

object Cart:
  val empty: Cart = Cart(Nil)

  @main def report(): Unit =
    val cart = empty.add(Item("apple", 120)).add(Item("pear", 95))
    println(s"\${cart.items.size} items, \${Price(cart.total).show}")
`,
  "core/src/main/scala/com/acme/shop/Price.scala": `package com.acme.shop

/** An amount in cents, shown in euros. */
final case class Price(cents: Long):
  def show: String = f"\${cents / 100}%d.\${cents % 100}%02d EUR"

object Price:
  def of(euros: BigDecimal): Price =
    Price((euros * 100).setScale(0, BigDecimal.RoundingMode.HALF_UP).toLong)
`,
  "core/src/test/scala/com/acme/shop/CartSpec.scala": `package com.acme.shop

import org.scalatest.flatspec.AnyFlatSpec
import org.scalatest.matchers.should.Matchers
import org.scalatest.wordspec.AnyWordSpec

class CartSpec extends AnyFlatSpec with Matchers {
  "A Cart" should "add items" in {
    val cart = Cart.empty.add(Item("apple", 120))
    cart.items should have size 1
  }

  it should "sum the total" in {
    Cart.empty.add(Item("apple", 120)).total shouldBe 120
  }

  it must "reject negative quantities" in {
    an[IllegalArgumentException] should be thrownBy Item("pear", -1)
  }
}

/* WordSpec style: "nested" { clauses } */
class CartWordSpec extends AnyWordSpec with Matchers {
  "A Cart" when {
    "empty" should {
      "have no items" in {
        Cart.empty.items shouldBe empty
      }
    }
  }
}

class CheckoutSpec extends org.scalatest.funspec.AnyFunSpec {
  describe("Checkout") {
    it("applies the discount") {
      assert(Checkout(Cart.empty).discount == 0)
    }
  }
  it("charges shipping") { assert(true) }
}
`,
  "core/src/test/scala/com/acme/shop/PriceSuite.scala": `package com.acme.shop

import munit.FunSuite

class PriceSuite extends FunSuite {
  test("formats euros") {
    assertEquals(Price(1250).show, "12.50 EUR")
  }

  test("rounds half up") {
    assertEquals(Price.of(BigDecimal("0.125")).cents, 13L)
  }
}
`,
  "modules/api/src/main/scala/com/acme/shop/api/Main.scala": `package com.acme.shop.api

import com.acme.shop.{Cart, Item}

object Main {
  def main(args: Array[String]): Unit = {
    val port = args.headOption.map(_.toInt).getOrElse(8080)
    val cart = Cart.empty.add(Item("apple", 120))
    println(s"Serving the shop API on http://localhost:$port (\${cart.items.size} demo item)")
  }
}
`,
};

/** What `sbt projects "show scalaVersion" "show baseDirectory" "show libraryDependencies"` printed. */
export const sbtImportOutput = `[info] welcome to sbt 1.10.5 (Eclipse Adoptium Java 21.0.5)
[info] loading settings for project shop-build from plugins.sbt ...
[info] loading project definition from /home/anna/Projects/shop/project
[info] loading settings for project root from build.sbt ...
[info] set current project to shop (in build file:/home/anna/Projects/shop/)
[info] In file:/home/anna/Projects/shop/
[info] 	   api
[info] 	   core
[info] 	 * root
[info] api / scalaVersion
[info] 	2.13.15
[info] core / scalaVersion
[info] 	3.3.4
[info] scalaVersion
[info] 	3.3.4
[info] api / baseDirectory
[info] 	/home/anna/Projects/shop/modules/api
[info] core / baseDirectory
[info] 	/home/anna/Projects/shop/core
[info] baseDirectory
[info] 	/home/anna/Projects/shop
[info] api / libraryDependencies
[info] 	* org.scala-lang:scala-library:2.13.15
[info] 	* org.scalameta:munit:1.0.2:test
[info] 	* com.softwaremill.sttp.tapir:tapir-core:1.11.7
[info] 	* ch.qos.logback:logback-classic:1.5.12:runtime
[info] core / libraryDependencies
[info] 	* org.scala-lang:scala3-library:3.3.4
[info] 	* org.scalameta:munit:1.0.2:test
[info] 	* org.typelevel:cats-core:2.12.0
[info] 	* org.scalatest:scalatest:3.2.19:test
[info] libraryDependencies
[info] 	* org.scala-lang:scala3-library:3.3.4
[success] Total time: 0 s, completed Oct 27, 2026, 10:14:03 AM
`;
