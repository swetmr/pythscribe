<!-- exp=baseline-haiku task=macro_course_cards cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0425363 -->
```python
"use client"

from pyths.react import component, use_state

@component
def CourseCatalog():
    courses_data = [
        {"id": 1, "title": "Python for Beginners", "provider": "Coursera", "progress": 100},
        {"id": 2, "title": "Web Development with React", "provider": "Udemy", "progress": 75},
        {"id": 3, "title": "Machine Learning Basics", "provider": "Coursera", "progress": 50},
        {"id": 4, "title": "Advanced CSS", "provider": "Codecademy", "progress": 100},
        {"id": 5, "title": "Data Science Fundamentals", "provider": "edX", "progress": 30},
        {"id": 6, "title": "JavaScript Mastery", "provider": "Udemy", "progress": 85},
    ]
    
    current_tab, set_current_tab = use_state("All")
    enrolled, set_enrolled = use_state({1, 2, 4})
    
    def toggle_enroll(course_id):
        if course_id in enrolled:
            set_enrolled(enrolled - {course_id})
        else:
            set_enrolled(enrolled | {course_id})
    
    def get_filtered_courses():
        if current_tab == "All":
            return courses_data
        elif current_tab == "Enrolled":
            return [c for c in courses_data if c["id"] in enrolled]
        elif current_tab == "Completed":
            return [c for c in courses_data if c["id"] in enrolled and c["progress"] == 100]
        return courses_data
    
    filtered = get_filtered_courses()
    
    return div(class_name="catalog-container",
        div(class_name="tabs",
            button(on_click=lambda: set_current_tab("All"), 
                   class_name="tab-button" + (" active" if current_tab == "All" else ""),
                   "All"),
            button(on_click=lambda: set_current_tab("Enrolled"),
                   class_name="tab-button" + (" active" if current_tab == "Enrolled" else ""),
                   "Enrolled"),
            button(on_click=lambda: set_current_tab("Completed"),
                   class_name="tab-button" + (" active" if current_tab == "Completed" else ""),
                   "Completed"),
        ),
        div(class_name="courses-grid",
            *[div(class_name="course-card", key=c["id"],
                div(class_name="course-title", c["title"]),
                div(class_name="course-provider", c["provider"]),
                div(class_name="progress-bar-container",
                    div(class_name="progress-bar",
                        style={"width": f"{c['progress']}%"}),
                ),
                div(class_name="progress-text", f"{c['progress']}%"),
                button(
                    on_click=lambda cid=c["id"]: toggle_enroll(cid),
                    class_name="enroll-button",
                    "Unenroll" if c["id"] in enrolled else "Enroll"
                ),
            ) for c in filtered]
        ),
    )
```
