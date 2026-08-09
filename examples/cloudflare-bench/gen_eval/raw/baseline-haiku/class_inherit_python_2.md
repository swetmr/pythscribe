<!-- exp=baseline-haiku task=class_inherit cond=python sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0054854000000000005 -->
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

animal = Animal("Cat", "meow")
dog = Dog("Rex")
print(animal.speak())
print(dog.speak())
```
