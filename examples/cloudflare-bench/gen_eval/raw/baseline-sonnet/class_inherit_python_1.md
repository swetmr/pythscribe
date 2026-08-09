<!-- exp=baseline-sonnet task=class_inherit cond=python sample=1 model=claude-sonnet-5 cost_usd=0.012258 -->
This is a simple, self-contained coding task with no ambiguity — no skill needed here.

```python
class Animal:
    def __init__(self, name, sound):
        self.name = name
        self.sound = sound

    def speak(self):
        return f"{self.name} says {self.sound}"


class Dog(Animal):
    def __init__(self, name):
        super().__init__(name, "woof")


cat = Animal("Cat", "meow")
dog = Dog("Rex")
print(cat.speak())
print(dog.speak())
```
