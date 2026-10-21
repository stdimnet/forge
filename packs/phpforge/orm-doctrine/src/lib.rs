//! PHPForge Doctrine plugin: ORM mapping metadata and DQL support.
//!
//! - [`mapping`] reads entity mappings from PHP 8 attributes (`#[ORM\Column]`), legacy
//!   docblock annotations (`@ORM\Column`, parsed by the lang-php docblock parser), XML
//!   (`*.orm.xml`) and YAML (`*.orm.yml`) mapping files, and finds entity repositories.
//! - [`dql`] (from the pure `phpforge-dql` crate) tokenizes and parses DQL for highlighting, completion (entities, aliases,
//!   fields, associations, keywords and functions) and diagnostics.
//! - [`php`] finds DQL inside PHP: `createQuery('…')` (including concatenated strings and
//!   `Entity::class`) and QueryBuilder calls (`->where('o.status = :s')`), with the aliases
//!   the builder chain declares.

pub mod mapping;
pub mod php;

use std::path::Path;

use serde::{Deserialize, Serialize};

/// DQL tokenizing, parsing, highlighting, completion and diagnostics (pure crate
/// `phpforge-dql`, also built to WebAssembly).
pub use phpforge_dql::dql;
pub use phpforge_dql::model::{
    Association, AssociationKind, Entity, Field, Location, MappingSource, Repository, Snapshot,
};

/// Plugin settings (`doctrine.*`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// Directories scanned for annotated/attributed entities and repositories.
    pub source_directories: Vec<String>,
    /// Directories with `*.orm.xml` / `*.orm.yml` mapping files.
    pub mapping_directories: Vec<String>,
    /// Read legacy `@ORM\…` docblock annotations.
    pub annotations: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source_directories: vec!["src".into()],
            mapping_directories: vec![
                "config/doctrine".into(),
                "src/Resources/config/doctrine".into(),
            ],
            annotations: true,
        }
    }
}

/// Is Doctrine ORM installed or configured?
pub fn is_doctrine_project(root: &Path) -> bool {
    root.join("config/packages/doctrine.yaml").is_file()
        || root.join("vendor/doctrine/orm").is_dir()
        || std::fs::read_to_string(root.join("composer.json")).is_ok_and(|s| {
            s.contains("\"doctrine/orm\"") || s.contains("\"doctrine/doctrine-bundle\"")
        })
}

/// Scan the project for entity mappings and repositories.
pub fn scan(root: &Path, config: &Config) -> Snapshot {
    let mut entities = Vec::new();
    let mut repositories = Vec::new();
    for dir in &config.source_directories {
        let (e, r) = mapping::from_php_dir(root, dir, config.annotations);
        entities.extend(e);
        repositories.extend(r);
    }
    for dir in &config.mapping_directories {
        entities.extend(mapping::from_mapping_dir(root, dir));
    }
    entities.sort_by(|a, b| a.class.cmp(&b.class));
    entities.dedup_by(|a, b| a.class == b.class);
    // Link repositories to their entities in both directions.
    for r in &mut repositories {
        if r.entity.is_none() {
            r.entity = entities
                .iter()
                .find(|e| e.repository_class.as_deref() == Some(r.class.as_str()))
                .map(|e| e.class.clone());
        }
    }
    for e in &mut entities {
        if e.repository_class.is_none() {
            e.repository_class = repositories
                .iter()
                .find(|r| r.entity.as_deref() == Some(e.class.as_str()))
                .map(|r| r.class.clone());
        }
    }
    repositories.sort_by(|a, b| a.class.cmp(&b.class));
    Snapshot {
        entities,
        repositories,
    }
}

/// 0-based line and byte column of `offset` in `text`.
pub(crate) fn line_col(text: &str, offset: usize) -> (u32, u32) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32;
    let col = before.rfind('\n').map(|i| offset - i - 1).unwrap_or(offset) as u32;
    (line, col)
}

#[cfg(test)]
pub(crate) mod fixture {
    use std::fs;
    use std::path::Path;

    pub const ORDER: &str = r#"<?php

namespace App\Entity;

use App\Repository\OrderRepository;
use Doctrine\Common\Collections\ArrayCollection;
use Doctrine\Common\Collections\Collection;
use Doctrine\DBAL\Types\Types;
use Doctrine\ORM\Mapping as ORM;

#[ORM\Entity(repositoryClass: OrderRepository::class)]
#[ORM\Table(name: 'orders')]
class Order
{
    #[ORM\Id]
    #[ORM\GeneratedValue]
    #[ORM\Column]
    private ?int $id = null;

    #[ORM\Column(length: 32)]
    private string $status = 'new';

    #[ORM\Column(type: Types::DECIMAL, precision: 10, scale: 2)]
    private string $total = '0.00';

    #[ORM\Column(nullable: true)]
    private ?\DateTimeImmutable $paidAt = null;

    #[ORM\ManyToOne(inversedBy: 'orders')]
    #[ORM\JoinColumn(nullable: false)]
    private ?Customer $customer = null;

    /** @var Collection<int, OrderItem> */
    #[ORM\OneToMany(targetEntity: OrderItem::class, mappedBy: 'order', orphanRemoval: true)]
    private Collection $items;

    #[ORM\Embedded(class: Address::class)]
    private Address $shippingAddress;
}
"#;

    pub const CUSTOMER: &str = r#"<?php

namespace App\Entity;

use Doctrine\ORM\Mapping as ORM;

/**
 * @ORM\Entity(repositoryClass="App\Repository\CustomerRepository")
 * @ORM\Table(name="customers")
 */
class Customer
{
    /**
     * @ORM\Id
     * @ORM\GeneratedValue
     * @ORM\Column(type="integer")
     */
    private $id;

    /** @ORM\Column(type="string", length=180, unique=true) */
    private $email;

    /**
     * @ORM\OneToMany(targetEntity="Order", mappedBy="customer")
     */
    private $orders;
}
"#;

    pub const ADDRESS: &str = r#"<?php

namespace App\Entity;

use Doctrine\ORM\Mapping as ORM;

#[ORM\Embeddable]
class Address
{
    #[ORM\Column(length: 120)]
    private string $street;

    #[ORM\Column(length: 80)]
    private string $city;
}
"#;

    pub const ORDER_REPOSITORY: &str = r#"<?php

namespace App\Repository;

use App\Entity\Order;
use Doctrine\Bundle\DoctrineBundle\Repository\ServiceEntityRepository;
use Doctrine\Persistence\ManagerRegistry;

/**
 * @extends ServiceEntityRepository<Order>
 */
class OrderRepository extends ServiceEntityRepository
{
    public function __construct(ManagerRegistry $registry)
    {
        parent::__construct($registry, Order::class);
    }

    public function findPaid(): array
    {
        return $this->createQueryBuilder('o')
            ->join('o.customer', 'c')
            ->andWhere('o.status = :status')
            ->orderBy('o.paidAt', 'DESC')
            ->getQuery()
            ->getResult();
    }

    public function totals(): array
    {
        return $this->getEntityManager()->createQuery(
            'SELECT c.email, SUM(o.total) AS total
             FROM App\Entity\Order o
             JOIN o.customer c
             GROUP BY c.email'
        )->getResult();
    }
}
"#;

    pub const ITEM_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<doctrine-mapping xmlns="http://doctrine-project.org/schemas/orm/doctrine-mapping">
    <entity name="App\Entity\OrderItem" table="order_items">
        <id name="id" type="integer">
            <generator strategy="AUTO"/>
        </id>
        <field name="quantity" type="integer"/>
        <field name="price" type="decimal" precision="10" scale="2"/>
        <many-to-one field="order" target-entity="Order" inversed-by="items"/>
        <many-to-one field="product" target-entity="App\Entity\Product"/>
    </entity>
</doctrine-mapping>
"#;

    pub const PRODUCT_YML: &str = r#"App\Entity\Product:
    type: entity
    table: products
    repositoryClass: App\Repository\ProductRepository
    id:
        id:
            type: integer
            generator: { strategy: AUTO }
    fields:
        name:
            type: string
            length: 255
        price: { type: decimal, precision: 10, scale: 2 }
    manyToMany:
        tags:
            targetEntity: Tag
"#;

    pub fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    pub fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(r, "composer.json", r#"{"require":{"doctrine/orm":"^3.3"}}"#);
        write(r, "src/Entity/Order.php", ORDER);
        write(r, "src/Entity/Customer.php", CUSTOMER);
        write(r, "src/Entity/Address.php", ADDRESS);
        write(r, "src/Repository/OrderRepository.php", ORDER_REPOSITORY);
        write(r, "config/doctrine/OrderItem.orm.xml", ITEM_XML);
        write(r, "config/doctrine/Product.orm.yml", PRODUCT_YML);
        dir
    }

    pub fn snapshot() -> super::Snapshot {
        let dir = project();
        super::scan(dir.path(), &super::Config::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_all_mapping_sources() {
        let dir = fixture::project();
        assert!(is_doctrine_project(dir.path()));
        let snap = scan(dir.path(), &Config::default());
        let names: Vec<&str> = snap.entities.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Address", "Customer", "Order", "OrderItem", "Product"]
        );
        assert_eq!(
            snap.entity("App\\Entity\\Order")
                .unwrap()
                .repository_class
                .as_deref(),
            Some("App\\Repository\\OrderRepository")
        );
        assert_eq!(
            snap.entity_of_repository("App\\Repository\\OrderRepository")
                .unwrap()
                .name,
            "Order"
        );
        assert_eq!(snap.entity("App:Customer").unwrap().name, "Customer");
        assert_eq!(
            snap.entity("App\\\\Entity\\\\Product").unwrap().name,
            "Product"
        );
    }
}
