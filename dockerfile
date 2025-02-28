# syntax=docker/dockerfile:1
FROM python:3.13.2-bullseye AS runner 

WORKDIR /usr/src/scg_backend
COPY . . 

# install dependencies
RUN pip install poetry==2.1.1
RUN pip install -U "huggingface_hub[cli]"
# Pre-downlaod the model we are going to use in the applcation
# If we switch to using more / or different models then these should also be added to this list
RUN huggingface-cli download "deepseek-ai/DeepSeek-R1-Distill-Qwen-1.5B"

ENV POETRY_NO_INTERACTION=1 \
    POETRY_VIRTUALENVS_IN_PROJECT=1 \
    POETRY_VIRUTALENVS_CREATE=1 \
    POETRY_CACHE_DIR=/tmp/poetry_cache

RUN --mount=type=cache,target=$POETRY_CACHE_DIR poetry install --no-root

# This should be what the dockerfile runs
ENTRYPOINT ["poetry", "run", "fastapi", "run", "run.py"]
