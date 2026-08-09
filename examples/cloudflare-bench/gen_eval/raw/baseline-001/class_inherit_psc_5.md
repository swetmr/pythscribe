<!-- exp=baseline-001 task=class_inherit cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.0138665 -->
```python
class Animal:
    def __init__(self, name, sound):
        self.name = name
        self.sound = sound
    def speak(self):
        return f"{self.name} says {self.sound}"

class Dog(Animal):
    def __init__(self, name):
        self.name = name
        self.sound = "woof"

print(Animal("Cat", "meow").speak())
print(Dog("Rex").speak())
```
