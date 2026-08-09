class Animal {
    constructor(name, sound) {
        this.name = name;
        this.sound = sound;
    }
    speak() {
        return `${this.name} says ${this.sound}`;
    }
}
class Dog extends Animal {
    constructor(name) {
        this.name = name;
        this.sound = "woof";
    }
    fetch(item) {
        return `${this.name} fetches ${item}`;
    }
}
let dog = new Dog("Rex");
console.log(dog.speak());
console.log(dog.fetch("ball"));
