// Package configloader provides functionality to load and validate
// application configuration using Viper.
package configloader

import (
	"log"
	"reflect"
	"strings"

	"github.com/joho/godotenv"
	"github.com/spf13/viper"
)

type Config struct {
	Server   ServerConfig
	Database DatabaseConfig
	JWTConfig JWTConfig
}

type ServerConfig struct {
	Port string
}

type JWTConfig struct {
	JWTSecret string
	JWTExp int
	RefreshExp int
}

type DatabaseConfig struct {
	Host     string
	Port     int
	User     string
	Password string
	DBName   string
}

func LoadConfig() (*Config, error) {
	if err := godotenv.Load(); err != nil {
		log.Println("Info: .env file not found, proceeding with system environment variables")
	}

	v := viper.New()

	v.AutomaticEnv()
	v.SetEnvKeyReplacer(strings.NewReplacer(".", "_"))

	var cfg Config
	bindAllEnv(v, cfg)

	if err := v.Unmarshal(&cfg); err != nil {
		return nil, err
	}

	return &cfg, nil
}

func bindAllEnv(v *viper.Viper, i interface{}, parts ...string) {
	ift := reflect.TypeOf(i)

	for n := 0; n < ift.NumField(); n++ {
		field := ift.Field(n)
		tag := field.Tag.Get("mapstructure")
		
		if tag == "" {
			tag = strings.ToLower(field.Name)
		}

		path := append(parts, tag)
		if field.Type.Kind() == reflect.Struct {
			bindAllEnv(v, reflect.New(field.Type).Elem().Interface(), path...)
		} else {
			v.SetDefault(strings.Join(path, "."), nil)
		}
	}
}