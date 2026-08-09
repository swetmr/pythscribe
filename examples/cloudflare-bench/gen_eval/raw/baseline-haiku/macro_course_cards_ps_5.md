<!-- exp=baseline-haiku task=macro_course_cards cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.028546299999999997 -->
```python
"use client"

from pyths.react import component, use_state

@component
def CourseCatalog():
    courses = [
        {"id": 1, "title": "Python for Beginners", "provider": "Coursera", "progress": 45, "enrolled": True},
        {"id": 2, "title": "Web Development Fundamentals", "provider": "edX", "progress": 100, "enrolled": True},
        {"id": 3, "title": "Data Science Basics", "provider": "Udemy", "progress": 0, "enrolled": False},
        {"id": 4, "title": "Machine Learning 101", "provider": "Coursera", "progress": 75, "enrolled": True},
        {"id": 5, "title": "Advanced JavaScript", "provider": "Udacity", "progress": 30, "enrolled": False},
        {"id": 6, "title": "Cloud Computing with AWS", "provider": "A Cloud Guru", "progress": 100, "enrolled": True},
    ]
    
    active_tab, set_active_tab = use_state("All")
    enrolled_state, set_enrolled_state = use_state({c["id"]: c["enrolled"] for c in courses})
    
    def toggle_enrolled(course_id):
        updated = {**enrolled_state, course_id: not enrolled_state[course_id]}
        set_enrolled_state(updated)
    
    def filter_courses():
        filtered = []
        for course in courses:
            course_enrolled = enrolled_state[course["id"]]
            is_completed = course["progress"] == 100
            
            if active_tab == "All":
                filtered.append(course)
            elif active_tab == "Enrolled" and course_enrolled:
                filtered.append(course)
            elif active_tab == "Completed" and is_completed:
                filtered.append(course)
        
        return filtered
    
    filtered = filter_courses()
    
    return div(class_name="course-catalog",
        div(class_name="tabs",
            button(
                class_name=f"tab-button {'active' if active_tab == 'All' else ''}",
                on_click=lambda: set_active_tab("All"),
                "All"
            ),
            button(
                class_name=f"tab-button {'active' if active_tab == 'Enrolled' else ''}",
                on_click=lambda: set_active_tab("Enrolled"),
                "Enrolled"
            ),
            button(
                class_name=f"tab-button {'active' if active_tab == 'Completed' else ''}",
                on_click=lambda: set_active_tab("Completed"),
                "Completed"
            ),
        ),
        div(class_name="courses-grid",
            *[CourseCard(
                key=course["id"],
                course=course,
                is_enrolled=enrolled_state[course["id"]],
                on_toggle=lambda cid=course["id"]: toggle_enrolled(cid)
            ) for course in filtered]
        )
    )

@component
def CourseCard(course, is_enrolled, on_toggle):
    is_completed = course["progress"] == 100
    
    return div(class_name="course-card",
        h3(course["title"]),
        p(class_name="provider", course["provider"]),
        div(class_name="progress-container",
            div(
                class_name="progress-bar",
                style={"width": f"{course['progress']}%"}
            )
        ),
        p(class_name="progress-text", f"{course['progress']}%"),
        button(
            class_name="enroll-button",
            on_click=on_toggle,
            "Unenroll" if is_enrolled else "Enroll"
        ),
        p(class_name="completed-badge", "✓ Completed") if is_completed else None
    )
```
