<!-- exp=baseline-haiku task=class_inherit cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.004668499999999999 -->
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

cat = Animal("Cat", "meow")
dog = Dog("Rex")

print(cat.speak())
print(dog.speak())
```
