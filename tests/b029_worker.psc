W*

async def my_fetch(request):
    return Response("hello from worker", {"status": 200})

__default__ = handler(my_fetch)
