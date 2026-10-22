export class Cart {
  private items: Item[] = [];

  add(item: Item): void {
    this.items.push(item);
    console.log("added", item.id);
  }

  total(): number {
    let sum: number = 0;
    for (const item of this.items) {
      sum += item.price;
    }
    console.log(sum);
    return sum;
  }
}
