<!-- exp=baseline-sonnet task=class_inherit cond=python sample=4 model=claude-sonnet-5 cost_usd=0.0096855 -->
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
