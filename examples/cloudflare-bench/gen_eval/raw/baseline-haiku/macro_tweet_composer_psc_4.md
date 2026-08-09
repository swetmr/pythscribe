<!-- exp=baseline-haiku task=macro_tweet_composer cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.0059678000000000005 -->
Using the Skill tool to check for relevant implementation guidance before building the component.

<tool_call>
name: Skill
arguments:
  skill: frontend-design
</tool_call>

```python
"use client"
R*

@c
def TweetFeed():
    tweets, set_tweets = us([
        {"id": 1, "author": "alice", "text": "Just shipped v2.0!", "likes": 42},
        {"id": 2, "author": "bob", "text": "PythScribe is incredible", "likes": 128},
        {"id": 3, "author": "carol", "text": "Building a tweet feed right now", "likes": 7},
    ])
    text, set_text = us("")
    
    char_count = len(text)
    remaining = 280 - char_count
    can_post = 0 < char_count <= 280
    
    def post():
        if can_post:
            new_tweet = {
                "id": max([t["id"] for t in tweets] + [0]) + 1,
                "author": "you",
                "text": text,
                "likes": 0,
            }
            set_tweets([new_tweet, *tweets])
            set_text("")
    
    def like_tweet(tweet_id):
        set_tweets([
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ])
    
    return div(cn="feed-container",
        div(cn="compose-section",
            textarea(
                value=text,
                oh=lambda e: set_text(e.target.value),
                ph="What's happening!?",
                cn="compose-textarea",
            ),
            div(cn="compose-footer",
                span(cn="char-count", f"{remaining} characters left"),
                button(
                    oc=lambda: post(),
                    dis=not can_post,
                    cn="post-button",
                    "Post",
                ),
            ),
        ),
        div(cn="tweets-list",
            *[div(cn="tweet", key=t["id"],
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(
                    oc=lambda tid=t["id"]: like_tweet(tid),
                    cn="like-button",
                    f"❤️ {t['likes']}",
                ),
            ) for t in tweets],
        ),
    )
```
