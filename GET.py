import requests

url="http://gamblitz-lanplay.herokuapp.com:11451/info"

r = requests.get(url)

data = r.text

print(data)