FROM python:3.12-alpine

RUN apk add --no-cache git docker-cli docker-cli-compose

WORKDIR /app
COPY manager.py .

EXPOSE 9090

CMD ["python", "manager.py"]
