<!-- exp=baseline-sonnet task=class_inherit cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.0113886 -->
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

a = Animal("Cat", "meow")
d = Dog("Rex")
print(a.speak())
print(d.speak())
```
