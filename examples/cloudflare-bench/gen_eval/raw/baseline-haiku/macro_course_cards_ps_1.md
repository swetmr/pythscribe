<!-- exp=baseline-haiku task=macro_course_cards cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0338703 -->
```python
"use client"
from pyths.react import component, use_state

@component
def CourseCatalog():
    courses_data = [
        {"id": 1, "title": "Python Fundamentals", "provider": "Coursera", "progress": 75, "enrolled": True},
        {"id": 2, "title": "Web Development with React", "provider": "Udemy", "progress": 100, "enrolled": True},
        {"id": 3, "title": "Data Science Basics", "provider": "edX", "progress": 0, "enrolled": False},
        {"id": 4, "title": "Advanced JavaScript", "provider": "Pluralsight", "progress": 50, "enrolled": True},
        {"id": 5, "title": "Machine Learning Mastery", "provider": "Coursera", "progress": 100, "enrolled": True},
        {"id": 6, "title": "Cloud Computing with AWS", "provider": "LinkedIn Learning", "progress": 25, "enrolled": False},
    ]
    
    courses, set_courses = use_state(courses_data)
    active_tab, set_active_tab = use_state("All")
    
    def toggle_enrolled(course_id):
        updated = []
        for course in courses:
            if course["id"] == course_id:
                updated.append({**course, "enrolled": not course["enrolled"]})
            else:
                updated.append(course)
        set_courses(updated)
    
    def get_filtered_courses():
        if active_tab == "All":
            return courses
        elif active_tab == "Enrolled":
            return [c for c in courses if c["enrolled"]]
        elif active_tab == "Completed":
            return [c for c in courses if c["progress"] == 100]
        return courses
    
    filtered = get_filtered_courses()
    
    return div(class_name="catalog-container",
        div(class_name="tabs",
            button(class_name=f"tab-button {'active' if active_tab == 'All' else ''}", 
                   on_click=lambda: set_active_tab("All"), "All"),
            button(class_name=f"tab-button {'active' if active_tab == 'Enrolled' else ''}", 
                   on_click=lambda: set_active_tab("Enrolled"), "Enrolled"),
            button(class_name=f"tab-button {'active' if active_tab == 'Completed' else ''}", 
                   on_click=lambda: set_active_tab("Completed"), "Completed"),
        ),
        div(class_name="courses-grid",
            *[div(class_name="course-card",
                  div(class_name="course-header",
                      h3(course["title"]),
                      p(class_name="provider", course["provider"]),
                  ),
                  div(class_name="progress-container",
                      div(class_name="progress-bar-bg",
                          div(class_name="progress-bar-fill",
                              style={"width": f"{course['progress']}%"}
                          )
                      ),
                      p(class_name="progress-text", f"{course['progress']}%"),
                  ),
                  button(
                      class_name="enroll-button",
                      on_click=lambda cid=course["id"]: toggle_enrolled(cid),
                      "Unenroll" if course["enrolled"] else "Enroll"
                  ),
              )
              for course in filtered]
        )
    )
```
