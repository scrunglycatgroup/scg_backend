FROM python:3.13.3-slim-bookworm

WORKDIR /code

RUN pip install poetry

EXPOSE 8000

ENV POETRY_NO_INTERACTION=1 \
    POETRY_VIRTUALENVS_IN_PROJECT=1 \
    POETRY_VIRTUALENVS_CREATE=1 \
    POETRY_CACHE_DIR=/tmp/poetry_cache

COPY ./pyproject.toml ./poetry.lock ./
COPY ./run.py ./run.py 

RUN poetry install --no-root && rm -rf $POETRY_CACHE_DIR

CMD ["poetry", "run", "fastapi", "run", "run.py"]
